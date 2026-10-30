"""Configure the disposable Windows CI desktop for native 3D input tests.

The Win32 DEVMODEW ABI is documented at
https://learn.microsoft.com/windows/win32/api/wingdi/ns-wingdi-devmodew
"""
import ctypes
import os


class DisplayMode(ctypes.Structure):
    _fields_ = [
        ("dmDeviceName", ctypes.c_uint16 * 32),
        ("dmSpecVersion", ctypes.c_uint16), ("dmDriverVersion", ctypes.c_uint16),
        ("dmSize", ctypes.c_uint16), ("dmDriverExtra", ctypes.c_uint16),
        ("dmFields", ctypes.c_uint32),
        ("dmPositionX", ctypes.c_int32), ("dmPositionY", ctypes.c_int32),
        ("dmDisplayOrientation", ctypes.c_uint32), ("dmDisplayFixedOutput", ctypes.c_uint32),
        ("dmColor", ctypes.c_int16), ("dmDuplex", ctypes.c_int16),
        ("dmYResolution", ctypes.c_int16), ("dmTTOption", ctypes.c_int16),
        ("dmCollate", ctypes.c_int16), ("dmFormName", ctypes.c_uint16 * 32),
        ("dmLogPixels", ctypes.c_uint16),
    ] + [(name, ctypes.c_uint32) for name in (
        "dmBitsPerPel", "dmPelsWidth", "dmPelsHeight", "dmDisplayFlags", "dmDisplayFrequency",
        "dmICMMethod", "dmICMIntent", "dmMediaType", "dmDitherType", "dmReserved1", "dmReserved2",
        "dmPanningWidth", "dmPanningHeight")]


class WindowsDisplay:
    def __init__(self):
        self.user32 = ctypes.WinDLL("user32", use_last_error=True)
        self.user32.EnumDisplaySettingsW.argtypes = [ctypes.c_wchar_p, ctypes.c_uint32,
                                                    ctypes.POINTER(DisplayMode)]
        self.user32.EnumDisplaySettingsW.restype = ctypes.c_int32
        self.user32.ChangeDisplaySettingsExW.argtypes = [ctypes.c_wchar_p, ctypes.POINTER(DisplayMode),
                                                        ctypes.c_void_p, ctypes.c_uint32, ctypes.c_void_p]
        self.user32.ChangeDisplaySettingsExW.restype = ctypes.c_int32

    def mode(self, index):
        mode = DisplayMode()
        mode.dmSize = ctypes.sizeof(mode)
        return mode if self.user32.EnumDisplaySettingsW(None, index, ctypes.byref(mode)) else None

    def size(self):
        mode = self.mode(0xFFFFFFFF)  # ENUM_CURRENT_SETTINGS, physical pixels.
        if mode is None:
            raise RuntimeError("Cannot read the Windows CI display mode")
        return mode.dmPelsWidth, mode.dmPelsHeight

    def modes(self):
        index = 0
        while (mode := self.mode(index)) is not None:
            if mode.dmBitsPerPel >= 32:
                yield mode.dmPelsWidth, mode.dmPelsHeight
            index += 1

    def change(self, size):
        mode = DisplayMode()
        mode.dmSize = ctypes.sizeof(mode)
        mode.dmFields = 0x1C0000  # DM_BITSPERPEL | DM_PELSWIDTH | DM_PELSHEIGHT.
        mode.dmBitsPerPel = 32
        mode.dmPelsWidth, mode.dmPelsHeight = size
        # Flags 0 changes the running display without persisting registry settings.
        result = self.user32.ChangeDisplaySettingsExW(None, ctypes.byref(mode), None, 0, None)
        if result != 0:
            raise RuntimeError(f"Windows CI display change failed: {result}")


def configure(display, minimum=(1600, 1000)):
    current = display.size()
    fits = lambda size: all(actual >= required for actual, required in zip(size, minimum))
    if fits(current):
        return current
    choices = [size for size in display.modes() if fits(size)]
    if not choices:
        raise RuntimeError(f"No supported CI display mode fits {minimum}; current mode is {current}")
    chosen = min(choices, key=lambda size: (size[0] * size[1], size))
    display.change(chosen)
    actual = display.size()
    if not fits(actual):
        raise RuntimeError(f"CI desktop remains too small after mode change: {actual}")
    return actual


def main():
    if os.name != "nt" or os.environ.get("GITHUB_ACTIONS") != "true":
        raise RuntimeError("This helper configures only a disposable Windows GitHub Actions desktop")
    print("Native CI desktop:", configure(WindowsDisplay()))


if __name__ == "__main__":
    main()
