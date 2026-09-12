#!/bin/sh
# Original build recipe; FFmpeg sources are unmodified and shipped alongside it.
set -eu
version=9.0.1
archive="ffmpeg-${version}.tar.xz"
checksum=cf38e0e28c7e5605942c4a77755349b0145804a397af37eb1fb4c77cb237f635
mkdir -p /build-audio /audio-output/sources /audio-output/bin
cd /build-audio
curl --fail --silent --show-error --max-time 120 \
    -o "${archive}" "https://ffmpeg.org/releases/${archive}"
printf '%s  %s\n' "${checksum}" "${archive}" | sha256sum -c -
tar -xJf "${archive}"
cd "ffmpeg-${version}"
./configure --disable-autodetect --disable-gpl --disable-nonfree \
    --disable-doc --disable-debug --disable-network --disable-x86asm \
    --disable-programs --enable-ffmpeg --disable-everything \
    --disable-avdevice --disable-swscale --disable-iconv \
    --enable-protocol=pipe --enable-demuxer=pcm_s16le,mpegts,aac \
    --enable-decoder=pcm_s16le,aac --enable-encoder=aac,pcm_s16le \
    --enable-parser=aac,h264 --enable-muxer=adts,pcm_s16le,mp4 \
    --enable-bsf=aac_adtstoasc,extract_extradata \
    --enable-filter=aresample,aformat,anull --enable-small
make -j2 ffmpeg
./ffmpeg -L > /audio-output/sources/LICENSE-BUILD.txt 2>&1
grep -q 'Lesser General Public License' /audio-output/sources/LICENSE-BUILD.txt
cp ffmpeg /audio-output/bin/ffmpeg
cp COPYING.LGPLv2.1 LICENSE.md /audio-output/sources/
cp ffbuild/config.mak /audio-output/sources/config.mak
cp /usr/local/bin/build-audio-codec /audio-output/sources/build-audio-codec.sh
cp "/build-audio/${archive}" /audio-output/sources/
printf '%s  %s\n' "${checksum}" "${archive}" > /audio-output/sources/SHA256SUMS
printf 'No source modifications; generated build configuration is included.\n' \
    > /audio-output/sources/CHANGES.txt
apk info -v > /audio-output/sources/build-packages.txt
