fn main() {
    prost_build::compile_protos(
        &[
            "src/id/ident.proto",
            "src/authority/authority.proto",
            "src/journal/entry/entry.proto",
            "src/time/timestamp.proto",
            "src/authn/event/authn.proto",
            "src/authz/event/authz.proto",
            "src/journal/event/journal.proto",
            "src/name/name.proto",
            "src/journal/transaction/memo/memo.proto",
        ],
        &["src"],
    )
    .expect("failed to compile protos")
}
