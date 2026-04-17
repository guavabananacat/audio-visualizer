.PHONY: setup dev

setup:
	git config core.hooksPath .githooks

dev:
	trunk serve
