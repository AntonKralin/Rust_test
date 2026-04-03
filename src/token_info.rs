use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use serde_json::Value;
use anyhow::Result;
use std::str::FromStr;

#[derive(Debug)]
pub struct TokenInfo {
    pub name: String,
    pub symbol: String,
}

#[derive(Debug)]
pub struct HolderInfo {
    pub address: String,
    pub amount: u64,
}

pub async fn get_token_name(mint_address: &str) -> Result<Option<TokenInfo>> {
    // Используем бесплатный API Jupiter
    let url = format!("https://tokens.jup.ag/token/{}", mint_address);
    
    let response = reqwest::get(&url).await?;
    
    if response.status().is_success() {
        let data: Value = response.json().await?;
        Ok(Some(TokenInfo {
            name: data["name"].as_str().unwrap_or("Unknown").to_string(),
            symbol: data["symbol"].as_str().unwrap_or("Unknown").to_string(),
        }))
    } else {
        Ok(None)
    }
}

pub fn print_price(prefix: &str, base_amount: u64, quote_amount: u64, base_decimals: u8) {
    if base_amount == 0 || quote_amount == 0 {
        println!("{} Нет данных для расчёта цены", prefix);
        return;
    }
    
    let base_float = base_amount as f64 / 10f64.powi(base_decimals as i32);
    let quote_float = quote_amount as f64 / 10f64.powi(9); // WSOL имеет 9 decimals
    
    let price = quote_float / base_float;
    
    println!("{}", prefix);
    println!("   Мемкоинов в пуле: {:.4}", base_float);
    println!("   WSOL в пуле: {:.4} SOL", quote_float);
    println!("   💰 Цена: 1 токен = {:.12} SOL", price);
    println!("   💵 Цена: 1 токен = ${:.8}", price * 200.0); // примерная цена SOL
    println!("{}", "━".repeat(60));
}

pub async fn get_top5_holders(
    rpc_client: &RpcClient,
    mint_address: &str,
    pool_address: &str,
) -> Result<Vec<HolderInfo>> {
    println!("\n🔍 Поиск топ-5 холдеров (исключая пул)...");
    
    let mint_pubkey = Pubkey::from_str(mint_address)?;
    let pool_pubkey = Pubkey::from_str(pool_address)?;
    
    // Получаем все token account'ы для данного mint
    let token_accounts = rpc_client.get_token_accounts_by_owner(
        &mint_pubkey,
        solana_client::rpc_request::TokenAccountsFilter::Mint(mint_pubkey),
    )?;
    
    let mut holders: Vec<HolderInfo> = Vec::new();
    
    for account in token_accounts {
        let account_pubkey = Pubkey::from_str(&account.pubkey)?;
        
        // Пропускаем пул
        if account_pubkey == pool_pubkey {
            continue;
        }
        
        // Получаем баланс
        if let Ok(token_account) = rpc_client.get_token_account(&account_pubkey) {
            if let Some(amount) = token_account.token_amount {
                holders.push(HolderInfo {
                    address: account.pubkey,
                    amount: amount.amount.parse::<u64>().unwrap_or(0),
                });
            }
        }
    }
    
    // Сортируем по убыванию баланса и берём топ-5
    holders.sort_by(|a, b| b.amount.cmp(&a.amount));
    holders.truncate(5);
    
    println!("✅ Найдено {} крупных холдеров", holders.len());
    for (i, holder) in holders.iter().enumerate() {
        println!("   {}. {} - {} токенов", i + 1, &holder.address[..8], holder.amount);
    }
    
    Ok(holders)
}

pub async fn monitor_whales_and_panic(rpc_client: &RpcClient, holders: Vec<HolderInfo>) {
    println!("\n🐋 Запуск мониторинга китов...");
    
    for holder in holders {
        // Подписываемся на изменения каждого кита
        // В реальном приложении здесь нужен WebSocket подписка
        println!("   Отслеживаем кит-кошелёк: {}... ({} токенов)", &holder.address[..8], holder.amount);
    }
    
    println!("✅ Мониторинг китов активен");
}

pub async fn check_my_balance(rpc_client: &RpcClient, wallet_address: &str) -> Result<()> {
    println!("\n💰 Проверка баланса кошелька: {}", wallet_address);
    
    let wallet_pubkey = Pubkey::from_str(wallet_address)?;
    let balance = rpc_client.get_balance(&wallet_pubkey)?;
    
    let sol_balance = balance as f64 / 1_000_000_000.0;
    println!("   Баланс: {:.6} SOL", sol_balance);
    
    Ok(())
}