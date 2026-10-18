#!/usr/bin/env bash
# 构建 macOS .app（Apple Silicon / Intel 通用：加 --target x86_64-apple-darwin 出双架构）
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release -p mcode-ui
APP="target/release/bundle/MCode_Desktop.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
# 内置资产（ecc-core agents/skills + pua-core skills）随包分发，运行时物化到 ~/.mcode/assets
cp -R assets "$APP/Contents/Resources/assets"
# 应用图标（scripts/make_icons.py 生成）
cp assets/brand/icon.icns "$APP/Contents/Resources/icon.icns"
cp target/release/MCode_Desktop "$APP/Contents/MacOS/"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>MCode_Desktop</string>
  <key>CFBundleIdentifier</key><string>dev.mcode.desktop</string>
  <key>CFBundleVersion</key><string>0.1.4</string>
  <key>CFBundleShortVersionString</key><string>0.1.4</string>
  <key>CFBundleExecutable</key><string>MCode_Desktop</string>
  <key>CFBundleIconFile</key><string>icon.icns</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSMinimumSystemVersion</key><string>10.15</string>
</dict></plist>
PLIST
codesign --force --deep --sign - "$APP" 2>/dev/null || true
echo "✓ 产出: $APP"
