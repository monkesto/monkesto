fn main() {
    println!("cargo:rerun-if-changed=src/proto");
    println!("cargo:rerun-if-changed=build.rs");

    prost_build::compile_protos(
        &[
            "src/proto/error.proto",
            "src/proto/ident.proto",
            "src/proto/event/authn.proto",
            "src/proto/event/authority.proto",
            "src/proto/event/authz.proto",
            "src/proto/event/journal.proto",
            "src/proto/event/timestamp.proto",
        ],
        &["src/"],
    )
    .expect("failed to compile protos")
}
