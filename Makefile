.PHONY: dev build css test clean

dev:
	npx tailwindcss -i ./style/input.css -o ./target/site/pkg/monkesto.css --watch &
	cargo watch -x run

css:
	npx tailwindcss -i ./style/input.css -o ./target/site/pkg/monkesto.css --minify

build: css
	cargo build --release -vv

test:
	cargo test --release

clean:
	rm -rf target/site
	cargo clean
