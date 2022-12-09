.PHONY: usage
usage:
	@echo "make [TASK]"
	@echo "  format     reformat code"
	@echo "  build      build release"
	@echo "  test       run all tests"
	@echo "  dev-serve  watch for changes and run server"
	@echo "  dev-test   watch for changes and run tests"

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

.PHONY: dev-serve
dev-serve:
	find src/ | entr -rs 'cargo build --release && target/release/nanobot serve'

.PHONY: dev-test
dev-test:
	find src/ test/ | entr -rs 'cargo test --release'
