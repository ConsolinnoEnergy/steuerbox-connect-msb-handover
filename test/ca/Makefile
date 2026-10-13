# Generate fresh short-lived local credentials and reset rotation state for every run.
.PHONY: ca-all ca-clean
ca-all: ca-clean
	bash gen.sh
ca-clean:
	rm -rf out
