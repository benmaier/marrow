.PHONY: build bundle clean

APP_PATH = target/release/bundle/osx/Marrow.app
PLIST = $(APP_PATH)/Contents/Info.plist
SHELL := /bin/bash

build:
	source "$$HOME/.cargo/env" && cargo build --release

bundle:
	source "$$HOME/.cargo/env" && cargo bundle --release
	@# Add document type associations for markdown files
	@plutil -insert CFBundleDocumentTypes -json '[ \
		{ \
			"CFBundleTypeName": "Markdown Document", \
			"CFBundleTypeRole": "Viewer", \
			"LSHandlerRank": "Alternate", \
			"LSItemContentTypes": ["net.daringfireball.markdown", "public.plain-text"], \
			"CFBundleTypeExtensions": ["md", "markdown", "mdown", "mkd"] \
		} \
	]' $(PLIST)
	@touch $(APP_PATH)
	@echo "Built $(APP_PATH)"

clean:
	cargo clean

install: bundle
	@rm -rf /Applications/Marrow.app
	@cp -r $(APP_PATH) /Applications/
	@echo "Installed to /Applications/Marrow.app"
