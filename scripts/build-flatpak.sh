#!/bin/bash
set -e

echo "🔧 Building Calendar Flatpak..."
echo

# Install runtime if needed
echo "📦 Ensuring Flatpak runtime is installed..."
flatpak remote-add --if-not-exists --user flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user -y flathub org.freedesktop.Platform//25.08
flatpak install --user -y flathub org.freedesktop.Sdk//25.08
flatpak install --user -y flathub org.freedesktop.Sdk.Extension.rust-stable//25.08

echo
echo "🏗️  Building Flatpak..."
flatpak-builder --user --install --force-clean build-dir dev.jadejitsu.apps.Calendar.yml

echo
echo "✅ Build complete!"
echo "Run with: flatpak run dev.jadejitsu.apps.Calendar"
