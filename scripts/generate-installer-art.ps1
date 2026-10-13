Add-Type -AssemblyName System.Drawing
$assetRoot = Join-Path $PSScriptRoot '../src-tauri/installer-assets'
New-Item -ItemType Directory -Force $assetRoot | Out-Null
function New-BrandImage([int]$width,[int]$height,[string]$name) {
 $bitmap = [System.Drawing.Bitmap]::new($width,$height)
 $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
 $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
 $rectangle = [System.Drawing.Rectangle]::new(0,0,$width,$height)
 $gradient = [System.Drawing.Drawing2D.LinearGradientBrush]::new($rectangle,[System.Drawing.Color]::FromArgb(25,29,41),[System.Drawing.Color]::FromArgb(56,42,85),45)
 $graphics.FillRectangle($gradient,$rectangle)
 $accent = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(188,170,244))
 $white = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(236,236,249))
 $muted = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(151,153,178))
 if ($height -gt 100) {
  $pen = [System.Drawing.Pen]::new([System.Drawing.Color]::FromArgb(43,197,180,255),1)
  for($i=0;$i -lt 7;$i++){ $graphics.DrawEllipse($pen,-65+($i*14),135+($i*14),260,260) }
  $graphics.FillRectangle($accent,22,39,31,4); $graphics.FillRectangle($accent,22,52,23,4); $graphics.FillRectangle($accent,22,65,15,4)
  $font = [System.Drawing.Font]::new('Segoe UI',25,[System.Drawing.FontStyle]::Bold)
  $small = [System.Drawing.Font]::new('Segoe UI',9)
  $graphics.DrawString('Flux',$font,$white,18,84)
  $graphics.DrawString('DOWNLOAD MANAGER',$small,$muted,20,126)
  $graphics.DrawString('1.4',$small,$accent,22,286)
  $pen.Dispose();$font.Dispose();$small.Dispose()
 } else {
  $font = [System.Drawing.Font]::new('Segoe UI',21,[System.Drawing.FontStyle]::Bold)
  $graphics.DrawString('Flux',$font,$white,62,9)
  $graphics.FillRectangle($accent,24,18,22,3);$graphics.FillRectangle($accent,24,27,16,3);$graphics.FillRectangle($accent,24,36,10,3)
  $font.Dispose()
 }
 $bitmap.Save((Join-Path $assetRoot $name),[System.Drawing.Imaging.ImageFormat]::Bmp)
 $accent.Dispose();$white.Dispose();$muted.Dispose();$gradient.Dispose();$graphics.Dispose();$bitmap.Dispose()
}
New-BrandImage 164 314 'sidebar.bmp'
New-BrandImage 150 57 'header.bmp'
