Synthetic H.264/AAC test media generated from FFmpeg testsrc2 and sine sources.
The eight `.jpg` files contain MPEG-TS segments, deliberately exercising misleading CDN suffixes.
No third-party footage or audio is included. Regenerate with a full test FFmpeg:

```
ffmpeg -f lavfi -i testsrc2=size=160x90:rate=12 -f lavfi -i sine=frequency=440:sample_rate=48000 -t 8 -c:v libx264 -preset ultrafast -g 12 -c:a aac -f hls -hls_time 1 -hls_list_size 0 -hls_segment_filename part%02d.jpg film.m3u8
```
