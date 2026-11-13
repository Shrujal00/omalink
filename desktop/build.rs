use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

fn main() {
    // Every .qml file in qml/ is part of the module; the two that hold shared
    // state and colors are singletons.
    let mut paths: Vec<_> = std::fs::read_dir("qml")
        .expect("qml directory")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "qml"))
        .collect();
    paths.sort();
    let qml: Vec<QmlFile> = paths
        .into_iter()
        .map(|p| {
            let singleton = matches!(p.file_stem().and_then(|s| s.to_str()), Some("Store" | "Theme"));
            QmlFile::from(p.to_string_lossy().into_owned()).singleton(singleton)
        })
        .collect();
    println!("cargo::rerun-if-changed=qml");

    CxxQtBuilder::new_qml_module(QmlModule::new("omalink").qml_files(qml))
        .qt_module("Gui")
        .qt_module("Quick")
        .qt_module("QuickControls2")
        .files(["src/bridge.rs"])
        .cpp_file("src/glshare.cpp")
        .build();
}
