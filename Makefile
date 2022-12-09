.PHONY: deps
deps:
	sudo apt-get install -y rustc cargo
	sudo apt-get install -y python3-pip
	sudo pip install tesh

.PHONY: format
format:
	cargo fmt

.PHONY: build
build: target/release/nanobot

target/release/nanobot:
	cargo build --release

.PHONY: test
test: target/release/nanobot
	cargo fmt --check
	cargo test --release
	PATH="$${PATH}:$$(pwd)/target/release"; tesh --debug false ./doc
