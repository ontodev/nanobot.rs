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
	npm install -g tree-sitter-cli
	cd .. && git clone https://github.com/ontodev/tree-sitter-sqlrest.git
	cd ../tree-sitter-sqlrest && tree-sitter generate

.PHONY: format
format:
	cargo fmt

.PHONY: build
build:
	cargo build --release

build/:
	mkdir -p $@

target/release/nanobot: src/
	cargo build --release

TEST_TABLES = ldtab prefix statement
TEST_TSVS = $(foreach T,${TEST_TABLES},src/resources/test_data/${T}.tsv)
src/resources/test_data/zfa_excerpt.db: ${TEST_TSVS}
	rm -f $@
	sqlite3 $@ ".mode tabs" \
	$(foreach T,${TEST_TABLES},".import src/resources/test_data/${T}.tsv ${T}")

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

.PHONY: penguins
penguins: target/release/nanobot examples/penguins/
	rm -rf build/penguins/
	mkdir -p build/penguins/
	cp -r examples/penguins/* build/penguins/
	mkdir -p build/penguins/src/data/
	cd build/penguins \
	&& python3 generate.py \
	&& ../../$< init \
	&& ../../$< serve

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
ARCH := x86_64-unknown-linux-musl
TARGET := build/nanobot-$(ARCH)

target/$(ARCH)/release/nanobot: src
	docker pull clux/muslrust:stable
	docker run \
		-v cargo-cache:/root/.cargo/registry \
		-v $$PWD:/volume \
		--rm -t clux/muslrust:stable \
		cargo build --release

.PHONY: musl
musl: target/$(ARCH)/release/nanobot src/ | build/

.PHONY: upload
upload: target/$(ARCH)/release/nanobot | build/
	cp $< $(TARGET)
	gh release upload --clobber v$(TODAY) $(TARGET)

.PHONY: release
release: target/$(ARCH)/release/nanobot | build/
	cp $< $(TARGET)
	gh release create --draft --prerelease \
		--title "$(TODAY) Alpha Release" \
		--generate-notes \
		v$(TODAY) $(TARGET)
