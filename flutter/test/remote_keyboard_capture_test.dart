import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_hbb/models/remote_keyboard_capture.dart';

void main() {
  late List<bool> transitions;
  late bool selected;
  late RemoteKeyboardCapture capture;
  setUp(() {
    transitions = [];
    selected = true;
    capture = RemoteKeyboardCapture(
      onCapture: transitions.add,
      isSelected: () => selected,
    );
  });

  test('background hover and stale focus callbacks cannot capture typing', () {
    capture.canvasFocus(true);
    expect(transitions, isEmpty);
    expect(capture.canForward, isFalse);
    capture.windowFocus(true);
    capture.canvasFocus(true);
    capture.windowFocus(false); // Other app, pointer still over canvas.
    capture.canvasFocus(true); // Delayed hover callback.
    capture.canvasFocus(false); // Unfocus callback from the old canvas.
    expect(transitions, [true, false]);
    expect(capture.canForward, isFalse);
    capture.windowFocus(true);
    expect(transitions, [true, false, true]);
  });

  test('minimize releases capture and returning to the same canvas resumes',
      () {
    capture.windowFocus(true);
    capture.canvasFocus(true);
    capture.windowFocus(false);
    capture.windowFocus(false);
    expect(transitions, [true, false]);
    capture.windowFocus(true);
    expect(transitions, [true, false, true]);
  });

  test('local toolbar or editor focus releases the remote keyboard', () {
    capture.windowFocus(true);
    capture.canvasFocus(true);
    capture.canvasFocus(false);
    capture.windowFocus(false);
    capture.windowFocus(true);
    expect(transitions, [true, false]);
    expect(capture.resumeCanvas, isFalse);
    capture.canvasFocus(true);
    expect(transitions.last, isTrue);
  });

  test('inactive tab cannot acquire or resume capture', () {
    capture.windowFocus(true);
    capture.canvasFocus(true);
    capture.windowFocus(false);
    selected = false;
    capture.windowFocus(true);
    capture.canvasFocus(true);
    expect(transitions, [true, false]);
    expect(capture.canForward, isFalse);
    expect(capture.resumeCanvas, isFalse);
  });

  test('closing releases held keys once and cannot resume', () {
    capture.windowFocus(true);
    capture.canvasFocus(true);
    capture.release();
    capture.release();
    capture.windowFocus(false);
    capture.windowFocus(true);
    expect(transitions, [true, false]);
  });
}
