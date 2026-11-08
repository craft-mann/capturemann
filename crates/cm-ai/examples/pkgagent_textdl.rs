//! Throwaway (not committed): downloads a text model with the product's store, into the given root.
use cm_ai::text_model::store::TextModelStore;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (store, warn) = TextModelStore::open(&a[1]);
    println!("warn: {warn:?}");
    let mut last = 0u64;
    let out = store
        .download(&a[2], &mut |p| {
            let s = format!("{p:?}");
            if s.len() as u64 != last || true {
                last = s.len() as u64;
            }
            eprintln!("{s}");
            true
        })
        .unwrap();
    println!("outcome: {out:?}");
    store.select(&a[2]).unwrap();
    println!("installed: {}", store.is_installed(&a[2]));
}
