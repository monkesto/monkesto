fn main() {
    prost_build::compile_protos(
        &[
            "src/id/ident.proto",
            "src/id/ident_error.proto",
            "src/authority/authority.proto",
            "src/journal/entry/entry.proto",
            "src/time/timestamp.proto",
            "src/authn/event/authn.proto",
            "src/authz/event/authz.proto",
            "src/journal/event/journal.proto",
            "src/error/proto_decode_error.proto",
            "src/error/monkesto_error.proto",
            "src/name/name.proto",
            "src/name/name_error.proto",
            "src/journal/error/journal_error.proto",
            "src/authn/error/user_error.proto",
            "src/authn/passkey/error/passkey_error.proto",
            "src/journal/transaction/memo/memo.proto",
            "src/journal/transaction/memo/memo_error.proto",
        ],
        &["src"],
    )
    .expect("failed to compile protos")
}
