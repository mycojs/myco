.PHONY: all clean debug release runtime run

release: target/release/myco

debug: target/debug/myco

runtime: runtime/index.js

run: runtime
	cd init && \
	cargo run run

target/release/myco: runtime
	cargo build --release

target/debug/myco: runtime
	cargo build

runtime/index.js: runtime/node_modules runtime/index.ts
	cd runtime && \
	npm run build

runtime/node_modules: runtime/package.json
	cd runtime && \
	npm install && \
	touch node_modules

clean:
	rm -rf target && \
	rm -rf runtime/node_modules && \
	rm -f runtime/index.js
	rm -f runtime/index.js.map
