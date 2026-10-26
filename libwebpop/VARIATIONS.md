# Variations from libwebp

libwebpop omits libwebp's `OptimizeSingleFrame` finalization step, which decodes
and re-encodes a remaining cropped animation frame as a full-canvas still image
and selects that file if it is smaller. This applies when optimized animation
encoding produces exactly one output frame whose rectangle is smaller than the
canvas, either from one submitted input or after repeated inputs merge. libwebpop
retains the valid single-frame animation in these cases, so its output can be
larger and differ from libwebp's final bytes. This omission keeps production
decoding outside the encoder's scope. Ordinary full-canvas single-frame mux
cleanup still converts to a still image without decoding.
