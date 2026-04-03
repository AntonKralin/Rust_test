mod config;
mod token_info;

use solana_client::{
    rpc_client::RpcClient,
    rpc_config::RpcAccountInfoConfig,
    nonblocking::pubsub_client::PubsubClient as NonblockingPubsubClient,
};
use solana_sdk::{
    pubkey::Pubkey,
    commitment_config::CommitmentConfig,
};
use tokio::time::{Duration};
use std::str::FromStr;
use dotenv::dotenv;
use std::sync::Arc;
use tokio::sync::Mutex;
use anyhow::Result;

use config::bot_config;
use token_info::{
    get_token_name, print_price, get_top5_holders, monitor_whales_and_panic, check_my_balance,
};

// PumpSwap AMM Pool layout (после 8-байтового discriminator):
const POOL_BASE_MINT_OFFSET: usize = 43;
const POOL_QUOTE_MINT_OFFSET: usize = 75;
const POOL_BASE_VAULT_OFFSET: usize = 139;
const POOL_QUOTE_VAULT_OFFSET: usize = 171;

// SPL Token Account layout:
const SPL_AMOUNT_OFFSET: usize = 64;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    println!("🚀 Запуск мониторинга цены токена (PumpSwap AMM)...\n");

    let helius_key = std::env::var("HELIUS_API_KEY")
        .expect("❌ HELIUS_API_KEY не найден в .env файле!");

    let rpc_url = format!("https://mainnet.helius-rpc.com/?api-key={}", helius_key);
    let ws_url = format!("wss://mainnet.helius-rpc.com/?api-key={}", helius_key);

    let rpc_client = RpcClient::new_with_commitment(rpc_url, CommitmentConfig::confirmed());
    
    println!("✅ Подключение создано\n");

    let config = bot_config();
    check_my_balance(&rpc_client, &config.my_wallet_address).await?;

    // Шаг 1: загружаем данные пула
    let pool_address = Pubkey::from_str(&config.target_pool_address)?;
    println!("🎯 Пул: {}", pool_address);
    println!("📥 Загружаем данные пула...");

    let pool_account_info = rpc_client.get_account(&pool_address)?;
    let pool_data = &pool_account_info.data;
    println!("📏 Размер данных пула: {} байт", pool_data.len());

    // Шаг 2: извлекаем адреса vault-аккаунтов
    let base_mint_address = Pubkey::new_from_array(
        pool_data[POOL_BASE_MINT_OFFSET..POOL_BASE_MINT_OFFSET + 32]
            .try_into()
            .unwrap()
    );
    let _quote_mint_address = Pubkey::new_from_array(
        pool_data[POOL_QUOTE_MINT_OFFSET..POOL_QUOTE_MINT_OFFSET + 32]
            .try_into()
            .unwrap()
    );
    let base_vault_address = Pubkey::new_from_array(
        pool_data[POOL_BASE_VAULT_OFFSET..POOL_BASE_VAULT_OFFSET + 32]
            .try_into()
            .unwrap()
    );
    let quote_vault_address = Pubkey::new_from_array(
        pool_data[POOL_QUOTE_VAULT_OFFSET..POOL_QUOTE_VAULT_OFFSET + 32]
            .try_into()
            .unwrap()
    );

    println!("\n🏷️  Base Mint  (мемкоин): {}", base_mint_address);
    println!("🏦 Base Vault  (мемкоин): {}", base_vault_address);
    println!("🏦 Quote Vault (WSOL):    {}", quote_vault_address);

    // Шаг 3: получаем информацию о токене
    let token_name = get_token_name(&base_mint_address.to_string()).await?;
    println!("TokenName: {:?}", token_name.as_ref().map(|n| &n.name));

    // Узнаём decimals base-токена
    println!("\n🔍 Получаем decimals токена...");
    let base_mint_info = rpc_client.get_account(&base_mint_address)?;
    let base_decimals = if base_mint_info.data.len() >= 44 {
        base_mint_info.data[44] as u8 // Mint layout: decimals на offset 44
    } else {
        6u8
    };
    println!("📊 Base decimals: {}", base_decimals);
    println!("📊 Quote decimals: 9 (WSOL всегда 9)");

    // Получаем топ-5 холдеров
    let top5_vaults = get_top5_holders(&rpc_client, &base_mint_address.to_string(), &pool_address.to_string()).await?;

    // Мониторим китов
    monitor_whales_and_panic(&rpc_client, top5_vaults).await;

    // Шаг 4: читаем начальные балансы
    let read_amount = |data: &[u8]| -> u64 {
        if data.len() >= SPL_AMOUNT_OFFSET + 8 {
            u64::from_le_bytes(data[SPL_AMOUNT_OFFSET..SPL_AMOUNT_OFFSET + 8].try_into().unwrap())
        } else {
            0
        }
    };

    let base_vault_info = rpc_client.get_account(&base_vault_address)?;
    let quote_vault_info = rpc_client.get_account(&quote_vault_address)?;
    
    let mut base_amount = read_amount(&base_vault_info.data);
    let mut quote_amount = read_amount(&quote_vault_info.data);

    print_price("📊 Начальная цена:", base_amount, quote_amount, base_decimals);

    // Шаг 5: подписываемся на изменения через WebSocket
    println!("\n⏳ Подписываемся на изменения vault-аккаунтов...");

    let pubsub_client = NonblockingPubsubClient::connect(&ws_url).await?;
    
    let base_amount_arc = Arc::new(Mutex::new(base_amount));
    let quote_amount_arc = Arc::new(Mutex::new(quote_amount));
    
    let (base_amount_clone, quote_amount_clone) = (base_amount_arc.clone(), quote_amount_arc.clone());
    let (base_amount_print, quote_amount_print) = (base_amount_arc.clone(), quote_amount_arc.clone());
    
    // Таймер для дебаунса
    let print_timer = Arc::new(Mutex::new(None::<tokio::time::Instant>));
    let print_timer_clone = print_timer.clone();
    
    // Обработчик для Base Vault
    let base_vault_pubkey = base_vault_address;
    let base_decimals_clone = base_decimals;
    let base_subscription = pubsub_client.account_subscribe(
        &base_vault_pubkey,
        Some(RpcAccountInfoConfig {
            commitment: Some(CommitmentConfig::confirmed()),
            ..Default::default()
        }),
        move |response| {
            let base_amount_clone = base_amount_clone.clone();
            let quote_amount_clone = quote_amount_clone.clone();
            let print_timer = print_timer_clone.clone();
            let base_decimals = base_decimals_clone;
            
            Box::new(async move {
                if let Some(account) = response.value {
                    let new_amount = if account.data.len() >= SPL_AMOUNT_OFFSET + 8 {
                        u64::from_le_bytes(account.data[SPL_AMOUNT_OFFSET..SPL_AMOUNT_OFFSET + 8].try_into().unwrap())
                    } else {
                        0
                    };
                    
                    println!("\n📨 Base Vault изменился (кто-то купил/продал мемкоин)");
                    {
                        let mut base = base_amount_clone.lock().await;
                        *base = new_amount;
                    }
                    
                    // Дебаунс: ждём оба обновления
                    let mut timer = print_timer.lock().await;
                    if timer.is_none() {
                        *timer = Some(tokio::time::Instant::now());
                        let base_am = base_amount_clone.clone();
                        let quote_am = quote_amount_clone.clone();
                        let decimals = base_decimals;
                        
                        tokio::spawn(async move {
                            tokio::time::sleep(Duration::from_millis(10)).await;
                            let base_val = *base_am.lock().await;
                            let quote_val = *quote_am.lock().await;
                            print_price("📈 Новая сделка (Синхронизировано):", base_val, quote_val, decimals);
                        });
                    }
                }
            })
        },
    ).await?;

    // Обработчик для Quote Vault
    let quote_vault_pubkey = quote_vault_address;
    let quote_subscription = pubsub_client.account_subscribe(
        &quote_vault_pubkey,
        Some(RpcAccountInfoConfig {
            commitment: Some(CommitmentConfig::confirmed()),
            ..Default::default()
        }),
        move |response| {
            let base_amount_clone = base_amount_print.clone();
            let quote_amount_clone = quote_amount_print.clone();
            let print_timer = print_timer_clone.clone();
            let base_decimals = base_decimals_clone;
            
            Box::new(async move {
                if let Some(account) = response.value {
                    let new_amount = if account.data.len() >= SPL_AMOUNT_OFFSET + 8 {
                        u64::from_le_bytes(account.data[SPL_AMOUNT_OFFSET..SPL_AMOUNT_OFFSET + 8].try_into().unwrap())
                    } else {
                        0
                    };
                    
                    println!("\n📨 Quote Vault изменился (WSOL зашёл/вышел)");
                    {
                        let mut quote = quote_amount_clone.lock().await;
                        *quote = new_amount;
                    }
                    
                    let mut timer = print_timer.lock().await;
                    if timer.is_none() {
                        *timer = Some(tokio::time::Instant::now());
                        let base_am = base_amount_clone.clone();
                        let quote_am = quote_amount_clone.clone();
                        let decimals = base_decimals;
                        
                        tokio::spawn(async move {
                            tokio::time::sleep(Duration::from_millis(10)).await;
                            let base_val = *base_am.lock().await;
                            let quote_val = *quote_am.lock().await;
                            print_price("📈 Новая сделка (Синхронизировано):", base_val, quote_val, decimals);
                        });
                    }
                }
            })
        },
    ).await?;

    println!("✅ Подписки активны. Ждём сделок...\n");
    println!("{}", "━".repeat(60));

    // Держим программу запущенной
    tokio::signal::ctrl_c().await?;
    println!("\n\n⏹️  Останавливаем мониторинг...");
    println!("👋 До встречи!");

    Ok(())
}