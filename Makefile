.PHONY: test fmt clippy security check demo bench docker-test docker-check ui

test:
	cargo test

fmt:
	cargo fmt --all -- --check

clippy:
	cargo clippy --all-targets -- -D warnings

security:
	sh scripts/security_check.sh

check:
	docker compose run --rm checks

demo:
	rm -rf warehouse
	cargo run --bin nemo -- table create ./warehouse/events --schema examples/event_schema.json --graph-dim country --graph-dim date --graph-dim customer
	cargo run --bin nemo -- table append ./warehouse/events --file data/events-vn.parquet --records 100 --partition country=VN --partition date=2026-06 --partition customer=123
	cargo run --bin nemo -- table plan ./warehouse/events --predicate country=VN --predicate date=2026-06 --predicate customer=123
	cargo run --bin nemo -- table validate ./warehouse/events

bench:
	cargo run --bin nemo -- bench graph --countries 8 --dates 31 --customers 100 --files-per-leaf 2 --country C001 --date 2026-06-01 --customer cust-000001

docker-test:
	docker compose run --rm dev /usr/local/cargo/bin/cargo test --locked

docker-check:
	$(MAKE) check

ui:
	docker compose run --rm --service-ports dev /usr/local/cargo/bin/cargo run --bin nemo-ui
