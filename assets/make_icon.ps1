# Build LightBrowser's app icon from assets/app-icon-source.png (the Gemini render):
# find the blue rounded square, crop to it, make everything outside it transparent (drops the
# grey backdrop, drop shadow and watermark), and export icon-256.png + PNGs for icon.ico.
# Run: powershell -File assets/make_icon.ps1   (then: python assets/make_icon.py --from-pngs)
Add-Type -AssemblyName System.Drawing
$dir = $PSScriptRoot
$src = [System.Drawing.Bitmap]::FromFile((Join-Path $dir 'app-icon-source.png'))
$w = $src.Width; $h = $src.Height

# Bounding box of the blue tile: pixels clearly bluer than they are red.
$data = $src.LockBits((New-Object System.Drawing.Rectangle 0, 0, $w, $h), 'ReadOnly', 'Format32bppArgb')
$bytes = New-Object byte[] ($data.Stride * $h)
[System.Runtime.InteropServices.Marshal]::Copy($data.Scan0, $bytes, 0, $bytes.Length)
$src.UnlockBits($data)
$minX = $w; $minY = $h; $maxX = 0; $maxY = 0
for ($y = 0; $y -lt $h; $y += 2) {
    $row = $y * $data.Stride
    for ($x = 0; $x -lt $w; $x += 2) {
        $i = $row + $x * 4
        $b = $bytes[$i]; $r = $bytes[$i + 2]
        if ($b -gt 120 -and ($b - $r) -gt 60) {
            if ($x -lt $minX) { $minX = $x }; if ($x -gt $maxX) { $maxX = $x }
            if ($y -lt $minY) { $minY = $y }; if ($y -gt $maxY) { $maxY = $y }
        }
    }
}
$side = [Math]::Max($maxX - $minX, $maxY - $minY)
# Trim a hair inside the edge so the tile's own outer rim/antialiasing doesn't show.
$inset = [int]($side * 0.006)
$cx = ($minX + $maxX) / 2; $cy = ($minY + $maxY) / 2
$crop = New-Object System.Drawing.RectangleF ([float]($cx - $side / 2 + $inset)), ([float]($cy - $side / 2 + $inset)), ([float]($side - 2 * $inset)), ([float]($side - 2 * $inset))
"tile found at x=$minX..$maxX y=$minY..$maxY (side $side px)"

function RoundedSquare([int]$size, [float]$radiusFrac) {
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $r = $size * $radiusFrac; $d = 2 * $r
    $path.AddArc(0, 0, $d, $d, 180, 90)
    $path.AddArc($size - $d, 0, $d, $d, 270, 90)
    $path.AddArc($size - $d, $size - $d, $d, $d, 0, 90)
    $path.AddArc(0, $size - $d, $d, $d, 90, 90)
    $path.CloseFigure()
    return $path
}

# Master at 1024: paint the cropped tile through an antialiased rounded-square shape.
$master = New-Object System.Drawing.Bitmap 1024, 1024, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$g = [System.Drawing.Graphics]::FromImage($master)
$g.Clear([System.Drawing.Color]::Transparent)
$g.SmoothingMode = 'AntiAlias'; $g.InterpolationMode = 'HighQualityBicubic'; $g.PixelOffsetMode = 'HighQuality'
$scaled = New-Object System.Drawing.Bitmap 1024, 1024, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$gs = [System.Drawing.Graphics]::FromImage($scaled)
$gs.InterpolationMode = 'HighQualityBicubic'; $gs.PixelOffsetMode = 'HighQuality'
$gs.DrawImage($src, (New-Object System.Drawing.RectangleF 0, 0, 1024, 1024), $crop, 'Pixel')
$gs.Dispose()
$brush = New-Object System.Drawing.TextureBrush $scaled
$g.FillPath($brush, (RoundedSquare 1024 0.22))
$g.Dispose(); $brush.Dispose(); $scaled.Dispose(); $src.Dispose()
$master.Save((Join-Path $dir 'icon-1024.png'))

foreach ($s in 256, 48, 32, 16) {
    $b = New-Object System.Drawing.Bitmap $s, $s, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $gg = [System.Drawing.Graphics]::FromImage($b)
    $gg.InterpolationMode = 'HighQualityBicubic'; $gg.PixelOffsetMode = 'HighQuality'; $gg.SmoothingMode = 'AntiAlias'
    $gg.DrawImage($master, 0, 0, $s, $s)
    $gg.Dispose()
    $b.Save((Join-Path $dir "icon-$s.png"))
    $b.Dispose()
}
$master.Dispose()
"wrote icon-1024/256/48/32/16.png"
