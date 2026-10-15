#!/bin/sh
# Builds Airtight, installs it to ~/.local/bin and runs it as a systemd user
# service. PTP needs UDP ports 319 and 320, so sudo gives the binary the one
# capability to bind them.
#   ./install.sh
set -eu
cd "$(dirname "$0")"
bin="$HOME/.local/bin/airtight"
config="${XDG_CONFIG_HOME:-$HOME/.config}"
unit="$config/systemd/user/airtight.service"

cargo build -p airtight-app --release
systemctl --user stop airtight 2>/dev/null || true
mkdir -p "$(dirname "$bin")" "$(dirname "$unit")"
install -m 755 "${CARGO_TARGET_DIR:-target}/release/airtight" "$bin"
# The capability goes with the file: set it again after every copy.
sudo setcap cap_net_bind_service=+ep "$bin"

cat > "$unit" <<EOF
[Unit]
Description=Airtight: stream this computer's audio to an AirPlay 2 receiver
After=pipewire-pulse.service pulseaudio.service network-online.target

[Service]
ExecStart=$bin
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF

if ! grep -qs '"receiver_host": "' "$config/airtight/settings.json"; then
    echo "Looking for receivers..."
    "$bin" --list || true
    if [ -t 0 ]; then
        printf 'Receiver to stream to (name or IP): '
        read -r receiver
        "$bin" --use "$receiver"
    fi
fi

command -v playerctl >/dev/null || echo "Install playerctl to show the track on the receiver."
systemctl --user daemon-reload
systemctl --user enable airtight
systemctl --user restart airtight
echo "Installed $bin. Follow it: journalctl --user -u airtight -f"
