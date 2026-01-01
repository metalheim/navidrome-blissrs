cargo build --release --target wasm32-wasip1
cp target/wasm32-wasip1/release/navidrome_blissrs.wasm ./plugin.wasm
zip -j navidrome_blissrs.ndp manifest.json plugin.wasm
rm plugin.wasm