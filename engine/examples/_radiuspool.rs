
fn main() {
    for w in ["phantasma_prime", "strun", "torid", "shedu"] {
        let pool = wfsim_engine::mods_data::pool_for_weapon(w);
        let radius: Vec<&str> = pool.iter()
            .filter(|m| m.id.contains("firestorm") || m.id.contains("fulmination"))
            .map(|m| m.id).collect();
        println!("{w:18} blast-radius mods offered: {radius:?}");
    }
}
