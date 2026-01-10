.PHONY: build bundle clean install icon

APP_PATH = target/release/bundle/osx/Marrow.app
PLIST = $(APP_PATH)/Contents/Info.plist
ICON_SRC = icon/marrow5.png
ICONSET = icon/icon.iconset

build:
	cargo build --release

icon:
	@echo "Generating icon from $(ICON_SRC)..."
	@mkdir -p $(ICONSET)
	@sips -z 1024 1024 $(ICON_SRC) --out $(ICONSET)/icon_512x512@2x.png >/dev/null
	@sips -z 512 512 $(ICON_SRC) --out $(ICONSET)/icon_512x512.png >/dev/null
	@sips -z 512 512 $(ICON_SRC) --out $(ICONSET)/icon_256x256@2x.png >/dev/null
	@sips -z 256 256 $(ICON_SRC) --out $(ICONSET)/icon_256x256.png >/dev/null
	@sips -z 256 256 $(ICON_SRC) --out $(ICONSET)/icon_128x128@2x.png >/dev/null
	@sips -z 128 128 $(ICON_SRC) --out $(ICONSET)/icon_128x128.png >/dev/null
	@sips -z 64 64 $(ICON_SRC) --out $(ICONSET)/icon_32x32@2x.png >/dev/null
	@sips -z 32 32 $(ICON_SRC) --out $(ICONSET)/icon_32x32.png >/dev/null
	@sips -z 32 32 $(ICON_SRC) --out $(ICONSET)/icon_16x16@2x.png >/dev/null
	@sips -z 16 16 $(ICON_SRC) --out $(ICONSET)/icon_16x16.png >/dev/null
	@iconutil -c icns $(ICONSET) -o icon/icon.icns
	@rm -rf $(ICONSET)
	@echo "Generated icon/icon.icns"

bundle: icon
	cargo bundle --release
	@# Add document type associations for markdown and notebook files
	@plutil -insert CFBundleDocumentTypes -json '[ \
		{ \
			"CFBundleTypeName": "Markdown Document", \
			"CFBundleTypeRole": "Viewer", \
			"LSHandlerRank": "Alternate", \
			"LSItemContentTypes": ["net.daringfireball.markdown", "public.plain-text"], \
			"CFBundleTypeExtensions": ["md", "markdown", "mdown", "mkd"] \
		}, \
		{ \
			"CFBundleTypeName": "Jupyter Notebook", \
			"CFBundleTypeRole": "Viewer", \
			"LSHandlerRank": "Alternate", \
			"CFBundleTypeExtensions": ["ipynb"] \
		} \
	]' $(PLIST)
	@# Declare UTI for ipynb files
	@plutil -insert UTImportedTypeDeclarations -json '[ \
		{ \
			"UTTypeIdentifier": "com.marrow.jupyter-notebook", \
			"UTTypeDescription": "Jupyter Notebook", \
			"UTTypeConformsTo": ["public.json", "public.data"], \
			"UTTypeTagSpecification": { \
				"public.filename-extension": ["ipynb"] \
			} \
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
