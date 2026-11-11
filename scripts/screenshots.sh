#!/bin/sh
# Regenerate docs/screenshots/*.png from the real UI.
#
# The frames come from ratatui's TestBackend (an ignored unit test), so they use
# the actual draw code and palettes; a headless Chromium only turns the HTML
# into a PNG. Requires Chromium and ImageMagick, both only for this script.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

chromium=${CHROMIUM:-/Applications/Chromium.app/Contents/MacOS/Chromium}
if [ ! -x "$chromium" ]; then
    echo "screenshots: set CHROMIUM to a Chromium or Chrome binary" >&2
    exit 1
fi

cargo test --quiet write_readme_screenshots -- --ignored

dir=docs/screenshots
for html in "$dir"/*.html; do
    name=${html%.html}
    [ "$(basename "$name")" = report ] && continue
    "$chromium" --headless=new --disable-gpu --hide-scrollbars \
        --force-device-scale-factor=2 \
        --default-background-color=00000000 \
        --window-size=1200,700 \
        --screenshot="$name.png" "file://$root/$html" >/dev/null 2>&1
    magick "$name.png" -trim +repage "$name.png"
done

"$chromium" --headless=new --disable-gpu --hide-scrollbars \
    --force-device-scale-factor=2 \
    --window-size=1240,2600 \
    --screenshot="$dir/report.png" "file://$root/$dir/report.html" >/dev/null 2>&1
magick "$dir/report.png" -fuzz 1% -trim +repage \
    -bordercolor '#f6f4ef' -border 40 "$dir/report.png"
# The page is taller than the window; keep the header, the chart and the first
# habit card (with its notes), which is what fits beside the other README images.
magick "$dir/report.png" -gravity North -crop x3400+0+0 +repage "$dir/report.png"

rm -f "$dir"/*.html
echo "screenshots: wrote $(ls "$dir"/*.png | wc -l | tr -d ' ') PNGs"
