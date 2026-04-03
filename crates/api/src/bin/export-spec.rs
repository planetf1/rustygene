fn main() {
    let openapi = rustygene_api::openapi::openapi();
    println!(
        "{}",
        openapi
            .to_pretty_json()
            .expect("failed to serialize OpenAPI document")
    );
}
