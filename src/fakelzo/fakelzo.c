/*
 * Replacement for liblzo2 in the desktop bundles.
 *
 * WHY THIS EXISTS
 * ---------------
 * The bundles' only GPL-2.0-or-later component is liblzo2 (LZO, by Markus F.X.J. Oberhumer).
 * Nothing in node.in.net asks for it; it arrives through a chain of optional dependencies:
 *
 *     libgtk-4 -> libcairo-script-interpreter -> liblzo2
 *
 * libcairo-script-interpreter imports exactly two symbols from it, lzo2a_decompress and
 * lzo2a_999_compress, and uses them only when serialising drawing operations to a cairo
 * script — GTK debug machinery the application never drives.
 *
 * Since the loader still needs the library to exist (the interpreter links it), the options
 * were to rebuild GTK without the script interpreter, or to supply these two symbols
 * ourselves. This file does the latter: same names, same signatures, no LZO code,
 * MIT-licensed like the rest of the project. The GPL obligation on the bundles goes with it.
 *
 * TRADE-OFF, STATED PLAINLY
 * -------------------------
 * These functions do not compress anything — they report failure. If some future GTK version
 * starts exercising the cairo-script path, that feature degrades instead of working. If that
 * ever matters, the honest fixes in order of preference are: build GTK with the script
 * interpreter disabled (upstream supports it, the dependency is `required: false`), or
 * implement these two entry points over zlib, which round-trips consistently because the
 * interpreter both writes and reads the data itself.
 *
 * The built file keeps the name liblzo2 uses, because the interpreter records that name in
 * its import table / load commands. THIRD-PARTY-LICENSES.md says what the file actually is,
 * so nobody inspecting a bundle is misled.
 */
#include <stddef.h>

#define LZO_E_ERROR (-1)

int lzo2a_decompress(const unsigned char *src, size_t src_len,
                     unsigned char *dst, size_t *dst_len, void *wrkmem) {
    (void)src; (void)src_len; (void)dst; (void)wrkmem;
    if (dst_len) *dst_len = 0;
    return LZO_E_ERROR;
}

int lzo2a_999_compress(const unsigned char *src, size_t src_len,
                       unsigned char *dst, size_t *dst_len, void *wrkmem) {
    (void)src; (void)src_len; (void)dst; (void)wrkmem;
    if (dst_len) *dst_len = 0;
    return LZO_E_ERROR;
}
