name := 'xcalendar'
appid := 'dev.xarbit.apps.Calendar'

rootdir := ''
prefix := '/usr'

base-dir := absolute_path(clean(rootdir / prefix))

export INSTALL_DIR := base-dir / 'share'

bin-src := 'target' / 'release' / name
bin-dst := base-dir / 'bin' / name

desktop := appid + '.desktop'
desktop-src := 'res' / desktop
desktop-dst := clean(INSTALL_DIR / 'applications' / desktop)

metainfo := appid + '.metainfo.xml'
metainfo-src := 'res' / metainfo
metainfo-dst := clean(INSTALL_DIR / 'metainfo' / metainfo)

dbus-service := appid + '.service'
dbus-service-src := 'res' / dbus-service
dbus-service-dst := clean(INSTALL_DIR / 'dbus-1' / 'services' / dbus-service)

# Default recipe to display help information
default:
    @just --list

# Run the application in debug mode
run *args:
    cargo run --release -- {{args}}

# Run the application in debug mode with debug output
run-debug *args:
    cargo run -- {{args}}

# Build the application in release mode
build-release *args:
    cargo build --release {{args}}

# Build the application in debug mode
build-debug *args:
    cargo build {{args}}

# Check the project for errors
check *args:
    cargo check --all-features {{args}}

# Run tests
test *args:
    cargo test --all-features {{args}}

# Run clippy lints
clippy *args:
    cargo clippy --all-features {{args}}

# Format the code
fmt *args:
    cargo fmt --all {{args}}

# Install the application to the system
install:
    install -Dm0755 {{bin-src}} {{bin-dst}}
    install -Dm0644 {{desktop-src}} {{desktop-dst}}
    install -Dm0644 {{metainfo-src}} {{metainfo-dst}}
    install -Dm0644 {{dbus-service-src}} {{dbus-service-dst}}

# Install the application to the user prefix (~/.local), no root required.
# Creates any missing target directories and refreshes the desktop database.
install-user:
    @mkdir -p ~/.local/bin ~/.local/share/applications ~/.local/share/metainfo ~/.local/share/dbus-1/services
    install -Dm0755 {{bin-src}} ~/.local/bin/{{name}}
    # The desktop launcher's PATH does not include ~/.local/bin (only the
    # interactive shell rc files add it), so a bare `Exec=xcalendar` fails
    # when clicked. Rewrite it to the absolute installed path.
    install -Dm0644 {{desktop-src}} ~/.local/share/applications/{{desktop}}
    sed -i "s|^Exec=.*|Exec=$HOME/.local/bin/{{name}} %u|" ~/.local/share/applications/{{desktop}}
    install -Dm0644 {{metainfo-src}} ~/.local/share/metainfo/{{metainfo}}
    install -Dm0644 {{dbus-service-src}} ~/.local/share/dbus-1/services/{{dbus-service}}
    @update-desktop-database ~/.local/share/applications 2>/dev/null || true
    @echo "✅ Installed to ~/.local"

# Vendor dependencies
vendor:
    mkdir -p .cargo
    cargo vendor --sync Cargo.toml | head -n -1 > .cargo/config.toml
    echo 'directory = "vendor"' >> .cargo/config.toml

# Clean build artifacts
clean:
    cargo clean

# Generate Flatpak cargo dependencies manifest
flatpak-deps:
    python3 scripts/flatpak-cargo-generator.py Cargo.lock -o cargo-sources.json
    @echo "✅ Generated cargo-sources.json"

# Build Flatpak locally
flatpak-build:
    bash scripts/build-flatpak.sh

# Build standalone Flatpak bundle (.flatpak file)
flatpak-bundle:
    python3 scripts/flatpak-cargo-generator.py Cargo.lock -o cargo-sources.json
    @echo "✅ Generated cargo-sources.json"
    bash scripts/build-flatpak-bundle.sh

# Install standalone Flatpak bundle (.flatpak file)
flatpak-bundle-install:
    flatpak install --user dev.xarbit.apps.Calendar.flatpak

# Run Flatpak
flatpak-run *args:
    flatpak run {{appid}} {{args}}

# Uninstall local Flatpak
flatpak-uninstall:
    flatpak uninstall --user -y {{appid}} || true
    @echo "✅ Uninstalled {{appid}}"
