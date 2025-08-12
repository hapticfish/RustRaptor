// tools/keygen/src/main.rs
use base64::{engine::general_purpose::STANDARD as B64, Engine};
fn main() {
    sodiumoxide::init().unwrap();
    let (pk, sk) = sodiumoxide::crypto::box_::gen_keypair();      // 32-byte each :contentReference[oaicite:4]{index=4}
    println!("MASTER_PK_B64={}", B64.encode(pk.0));
    println!("MASTER_SK_B64={}", B64.encode(sk.0));
}


/*
    generate new keys run this in the cli and open master.env to get the keys

    cargo run -p keygen --release > master.env


*/