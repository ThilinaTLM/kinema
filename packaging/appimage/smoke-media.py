#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Decode a generated local WAV through the bundled libmpv, without a device.

This checks the media library/codec closure, not the Qt render API or GPU/audio
hardware. Those require a real-desktop embedded-player test before release.
"""
import ctypes as C
import math
from pathlib import Path
import struct
import sys
import tempfile
import time
import wave

root = Path(sys.argv[1])
library = next((root / 'usr/lib').glob('libmpv.so.*'))
mpv = C.CDLL(str(library))
mpv.mpv_create.restype = C.c_void_p
mpv.mpv_set_option_string.argtypes = [C.c_void_p, C.c_char_p, C.c_char_p]
mpv.mpv_initialize.argtypes = [C.c_void_p]
mpv.mpv_command.argtypes = [C.c_void_p, C.POINTER(C.c_char_p)]
mpv.mpv_get_property.argtypes = [C.c_void_p, C.c_char_p, C.c_int, C.c_void_p]
mpv.mpv_terminate_destroy.argtypes = [C.c_void_p]
handle = mpv.mpv_create()
assert handle, 'mpv_create failed'
try:
    for name, value in ((b'config', b'no'), (b'vo', b'null'), (b'ao', b'null'), (b'terminal', b'yes')):
        assert mpv.mpv_set_option_string(handle, name, value) >= 0, name
    assert mpv.mpv_initialize(handle) >= 0, 'mpv_initialize failed'
    with tempfile.TemporaryDirectory() as temporary:
        path = Path(temporary) / 'sample.wav'
        with wave.open(str(path), 'wb') as output:
            output.setparams((1, 2, 48000, 0, 'NONE', 'not compressed'))
            output.writeframes(b''.join(struct.pack('<h', int(4000 * math.sin(i * math.tau * 440 / 48000)))
                                       for i in range(48000 * 3)))
        command = (C.c_char_p * 3)(b'loadfile', str(path).encode(), None)
        assert mpv.mpv_command(handle, command) >= 0, 'loadfile failed'
        played = False
        for _ in range(100):
            position = C.c_double()
            # MPV_FORMAT_DOUBLE = 5
            if mpv.mpv_get_property(handle, b'time-pos', 5, C.byref(position)) >= 0 and position.value > 0.5:
                played = True
                break
            time.sleep(0.1)
        assert played, 'Bundled libmpv did not decode/advance local media'
        print('Bundled libmpv decoded and played local PCM audio successfully')
finally:
    mpv.mpv_terminate_destroy(handle)
