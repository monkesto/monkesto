.PHONY: dev build css test clean

all: dev

dev:
	npx tailwindcss -i ./style/input.css -o ./target/site/pkg/monkesto.css --watch &
	cargo watch -x run

css:
	npx tailwindcss -i ./style/input.css -o ./target/site/pkg/monkesto.css --minify

build: css
	cargo build --release

clean:
	rm -rf target/site
	cargo clean
