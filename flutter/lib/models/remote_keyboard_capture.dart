/// Window focus owns keyboard capture; pointer hover alone cannot acquire it.
/// Keep the canvas intent across application switches so returning to the same
/// session restores input without changing its key mapping or permissions.
class RemoteKeyboardCapture {
  RemoteKeyboardCapture({required this.onCapture, required this.isSelected});

  final void Function(bool) onCapture;
  final bool Function() isSelected;
  bool windowFocused = false;
  bool _canvasActive = false;
  bool _captured = false;

  bool get canForward => windowFocused && isSelected();
  bool get resumeCanvas => _canvasActive && isSelected();

  void windowFocus(bool focused) {
    windowFocused = focused;
    _sync();
  }

  void canvasFocus(bool active) {
    // Focus/hover notifications may arrive after the OS has blurred the window.
    // They must neither reacquire capture nor discard the previous focus intent.
    if (!canForward) return;
    _canvasActive = active;
    _sync();
  }

  void release() {
    _canvasActive = false;
    _sync();
  }

  void _sync() {
    final next = canForward && _canvasActive;
    if (next == _captured) return;
    _captured = next;
    onCapture(next);
  }
}
