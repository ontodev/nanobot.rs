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
	sudo apt-get update
	sudo apt-get install -y rustc cargo
	sudo apt-get install -y python3-pip
	sudo pip install tesh
	npm install -g tree-sitter-cli@0.21
	cd .. && git clone https://github.com/ontodev/tree-sitter-sqlrest.git
	cd ../tree-sitter-sqlrest && tree-sitter generate

.PHONY: format
format:
	cargo fmt

.PHONY: build
build:
	cargo build --release

build/ build/penguins/:
	mkdir -p $@

target/debug/nanobot: src/
	cargo build

target/release/nanobot: src/
	cargo build --release

TEST_TABLES = ldtab prefix statement
TEST_TSVS = $(foreach T,${TEST_TABLES},src/resources/test_data/${T}.tsv)
src/resources/test_data/zfa_excerpt.db: ${TEST_TSVS}
	rm -f $@
	sqlite3 $@ ".mode tabs" \
	$(foreach T,${TEST_TABLES},".import src/resources/test_data/${T}.tsv ${T}")

.PHONY: test
test: target/debug/nanobot build/penguins/.nanobot.db
	cargo fmt --check
	cargo test
	PATH="$${PATH}:$$(pwd)/target/debug"; tesh --debug false ./doc

.PHONY: dev-check
dev-check:
	find src/ tests/ | entr -rs 'cargo check --release'

.PHONY: dev-test
dev-test:
	find src/ tests/ | entr make test

.PHONY: serve
serve:
	find src/ | entr -rs 'cargo build --release && target/release/nanobot serve'

.PHONY: dev-serve
dev-serve:
	find src/ | entr -rs 'cargo build && target/debug/nanobot serve'

# First `cd src/javascript/` and `npm install`.
.PHONY: react
react:
	cargo build
	rm -rf build/react/
	mkdir -p build/react/assets/
	cd src/javascript/ \
	&& npm run build
	cp src/javascript/build/static/js/main.*.js build/react/assets/main.js
	cp src/javascript/build/static/css/main.*.css build/react/assets/main.css
	cd build/react/ \
	&& ../../target/debug/nanobot init \
	&& echo '' >> nanobot.toml \
	&& echo '[logging]' >> nanobot.toml \
	&& echo 'level = "DEBUG"' >> nanobot.toml \
	&& echo '' >> nanobot.toml \
	&& echo '[assets]' >> nanobot.toml \
	&& echo 'path = "assets/"' >> nanobot.toml \
	&& ../../target/debug/nanobot serve

build/penguins/%/.nanobot.db: target/debug/nanobot examples/penguins/% | build/penguins/%/
	rm -rf $|
	mkdir -p $|
	cp -r examples/penguins/* $|
	mkdir -p $|/src/data/
	cd $| \
	&& python3 generate.py \
	&& ../../$< init

.PHONY: penguins
penguins: target/debug/nanobot build/penguins/.nanobot.db
	cd build/penguins && ../../$< serve

build/synthea.zip: | build
	curl -L -o build/synthea.zip "https://synthetichealth.github.io/synthea-sample-data/downloads/synthea_sample_data_csv_apr2020.zip"

build/synthea/: build/synthea.zip examples/synthea/
	mkdir -p build/synthea/src/data
	cp -r examples/synthea/* build/synthea/
	unzip $< -d build/synthea/
	sed 's/,/	/g' build/synthea/csv/patients.csv > build/synthea/src/data/patients.tsv
	sed 's/,/	/g' build/synthea/csv/observations.csv > build/synthea/src/data/observations.tsv

# && ~/valve.rs/target/release/ontodev_valve src/schema/table.tsv .nanobot.db
.PHONY: synthea
synthea: target/release/nanobot
	rm -rf build/synthea/
	make build/synthea/
	cd build/synthea/ \
	&& time ../../$< init \
	&& ../../$< serve

TODAY := $(shell date +%Y-%m-%d)
YYYYMMDD := $(shell date +%Y%m%d)
ifeq ($(shell uname -m),arm64)
ARCH := aarch64
else
ARCH := x86_64
endif
ifeq ($(shell uname -s),Darwin)
TARGET := target/release/nanobot
BINARY := nanobot-v$(YYYYMMDD)-$(ARCH)-macos
else
TARGET := target/$(ARCH)-unknown-linux-musl/release/nanobot
BINARY := nanobot-v$(YYYYMMDD)-$(ARCH)-linux
endif
BINARY_PATH := build/$(BINARY)

# Build a Linux binary using Musl instead of GCC.
target/x86_64-unknown-linux-musl/release/nanobot: src/*.rs
	docker pull clux/muslrust:stable
	docker run \
		-v cargo-cache:/root/.cargo/registry \
		-v $$PWD:/volume \
		--rm -t clux/muslrust:stable \
		cargo build --release

.PHONY: musl
musl: target/x86_64-unknown-linux-musl/release/nanobot | build/

.PHONY: upload
upload: $(TARGET) | build/
	cp $< $(BINARY_PATH)
	gh release upload --clobber v$(TODAY) $(BINARY_PATH)

.PHONY: release
release: $(TARGET) | build/
	cp $< $(BINARY_PATH)
	gh release create --draft --prerelease \
		--title "$(TODAY) Alpha Release" \
		--generate-notes \
		v$(TODAY) $(BINARY_PATH)
	@echo "Please publish GitHub release v$(TODAY)"

