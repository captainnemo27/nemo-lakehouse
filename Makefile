.PHONY: test fmt clippy security demo docker-test

test:
	cargo test

fmt:
	cargo fmt --all -- --check

clippy:
	cargo clippy --all-targets -- -D warnings

security:
	sh scripts/security_check.sh

demo:
	rm -rf warehouse
	cargo run --bin nemo -- table create ./warehouse/events --schema examples/event_schema.json --graph-dim country --graph-dim date --graph-dim customer
	cargo run --bin nemo -- table append ./warehouse/events --file data/events-vn.parquet --records 100 --partition country=VN --partition date=2026-06 --partition customer=123
	cargo run --bin nemo -- table plan ./warehouse/events --predicate country=VN --predicate date=2026-06 --predicate customer=123

docker-test:
	docker compose run --rm dev cargo test

docker-check:
	docker compose run --rm checks
