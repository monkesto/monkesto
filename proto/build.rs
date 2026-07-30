fn main() {
    prost_build::compile_protos(
        &[
            "src/balance_update.proto",
            "src/error.proto",
            "src/ident.proto",
            "src/event/authn.proto",
            "src/event/authority.proto",
            "src/event/authz.proto",
            "src/event/journal.proto",
            "src/event/timestamp.proto",
        ],
        &["src"],
    )
    .expect("failed to compile protos")
}
