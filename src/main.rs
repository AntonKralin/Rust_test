mod mymath;

fn main() {
    println!("Hello, world!");
    let mut num: u32 = 50;
    num = num + 100;
    print!("Result {}", num);

    let mut numbers: [i32; 7] = [7, 4, 2, 5, 6, 1, 3];
    let mut numbers2 =  numbers.to_owned();

    numbers.sort();
    println!("numbers: {:?} ", numbers);

    let num_len = numbers2.len();
    for i in 0..num_len-1{
        for j in i+1..num_len{
            if numbers2[i] > numbers2[j]{
                let temp = numbers2[j];
                numbers2[j] = numbers2[i];
                numbers2[i] = temp;
            }
        }
    }
    println!("numbers2: {:?} ", numbers2);

    match num {
        1 => println!("One"),
        2 | 3 | 4 => println!("Two, Three or Four"),
        _ => println!("Something else"),
    }

    let mut user = String::from("name");
    greet_user(&mut user);
    println!("User: {}", user);

    println!("MyMath {}", mymath::add(1, 4));
    println!("MyMath {}", mymath::minus(1, 4));
}

fn greet_user(name: &mut String){
    *name = String::from("Fred");
}
