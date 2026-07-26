fn main() {
    prost_build::compile_protos(
        &[
            "src/proto/error.proto",
            "src/proto/event/authn.proto",
            "src/proto/event/authority.proto",
            "src/proto/event/authz.proto",
            "src/proto/event/journal.proto",
        ],
        &["src/"],
    )
    .expect("failed to compile protos")
}
