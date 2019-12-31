fn main() {
    println!("cargo:rerun-if-changed=../proto/data.proto");
    println!("cargo:rerun-if-changed=../proto/service.proto");

    let mut prost = prost_build::Config::new();

    prost.type_attribute(
        ".instantbible.data.VerseKey",
        "#[derive(serde::Serialize, Hash, Eq, Copy)]",
    );
    prost.type_attribute(
        ".instantbible.service.Response",
        "#[derive(serde::Serialize)]",
    );

    prost
        .compile_protos(
            &["../proto/data.proto", "../proto/service.proto"],
            &["../proto"],
        )
        .unwrap();
}
