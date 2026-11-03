fn main() {
    // sqlx::migrate! embeds the files at compile time, so new migrations must trigger a rebuild.
    println!("cargo:rerun-if-changed=migrations");
}
