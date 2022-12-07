deps:
	sudo apt-get install rustc cargo
	#Install tesh (requires python >= 3.9)
	sudo add-apt-repository ppa:deadsnakes/ppa
	sudo apt update
	sudo apt install python3.10 
	sudo apt install python3-pip
	sudo pip install tesh

format:
	cargo fmt

build:
	cargo build --release

test:
	cargo test
	tesh ./doc
