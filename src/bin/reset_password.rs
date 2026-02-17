// Password reset utility for Dockru
// Generates a bcrypt hash and prints the SQL command to run on your remote machine

use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 || args.len() > 3 {
        eprintln!("Dockru Password Reset Utility");
        eprintln!();
        eprintln!("Usage: {} <new_password> [database_path]", args[0]);
        eprintln!();
        eprintln!("Examples:");
        eprintln!("  cargo run --bin reset_password myNewPassword");
        eprintln!("  cargo run --bin reset_password myNewPassword /custom/path/dockru.db");
        eprintln!();
        process::exit(1);
    }

    let new_password = &args[1];
    let db_path = if args.len() == 3 {
        &args[2]
    } else {
        "/opt/dockru/data/dockru.db"
    };

    // Hash the password using bcrypt with cost 10 (matches BCRYPT_COST in auth.rs)
    let hash = match bcrypt::hash(new_password, 10) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("Error: Failed to hash password: {}", e);
            process::exit(1);
        }
    };

    println!("✓ Generated bcrypt hash for password: {}", new_password);
    println!();
    println!("Run this command on your remote machine:");
    println!();
    println!("  sqlite3 {} \"UPDATE user SET password = '{}' WHERE id = 1;\"", db_path, hash);
    println!();
    println!("Then log in with your new password: {}", new_password);
}
