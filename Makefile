.PHONY: usage
usage:
	@echo "make [TASK]"
	@echo "  format     reformat code"
	@echo "  build      build release"
	@echo "  test       run all tests"
	@echo "  dev-check  watch for changes and run cargo check"
	@echo "  dev-test   watch for changes and run tests"
	@echo "  dev-serve  watch for changes and run server"

.PHONY: deps
deps:
	sudo apt-get install -y rustc cargo
	sudo apt-get install -y python3-pip
	sudo pip install tesh
	sudo apt-get install -y tree-sitter
	cd .. && git clone https://github.com/ontodev/tree-sitter-sqlrest.git
	cd ../tree-sitter-sqlrest && tree-sitter generate

.PHONY: format
format:
	cargo fmt

.PHONY: build
build:
	cargo build --release

target/release/nanobot: src/
	cargo build --release

.PHONY: test
test:
	cargo fmt --check
	cargo test --release
	PATH="$${PATH}:$$(pwd)/target/release"; tesh --debug false ./doc

.PHONY: dev-check
dev-check:
	find src/ tests/ | entr -rs 'cargo check --release'

.PHONY: dev-test
dev-test:
	find src/ tests/ | entr make test

.PHONY: dev-serve
dev-serve:
	find src/ | entr -rs 'cargo build --release && target/release/nanobot serve'

