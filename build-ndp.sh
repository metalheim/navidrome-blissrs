cargo build --release --target wasm32-wasip1-threads
cp -f ./target/wasm32-wasip1-threads/release/navidrome_blissrs.wasm ./plugin.wasm
zip -j navidrome-blissrs.ndp manifest.json plugin.wasm
rm -f ./plugin.wasm
