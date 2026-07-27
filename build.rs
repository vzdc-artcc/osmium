fn main() {
    // `sqlx::migrate!("./migrations")` embeds the migration files at compile time,
    // but cargo does not otherwise treat them as build inputs — so adding a new
    // migration would not trigger a recompile and the new migration would be
    // silently missing from the binary. Track the directory to force a rebuild
    // whenever migrations change.
    println!("cargo:rerun-if-changed=migrations");
}
