fn main() {
    prost_build::compile_protos(&["src/proto/error.proto"], &["src/"])
        .expect("failed to compile protos")
}
