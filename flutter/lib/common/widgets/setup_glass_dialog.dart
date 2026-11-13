import 'dart:ui' as ui;
import 'package:flutter/material.dart';

/// Blur the content behind the setup sheet, never its text or controls.
class SetupGlassDialog extends StatelessWidget {
  final Widget title;
  final Widget content;
  final List<Widget> actions;
  final double maxWidth;

  const SetupGlassDialog({
    super.key,
    required this.title,
    required this.content,
    required this.actions,
    this.maxWidth = 520,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final highContrast = MediaQuery.highContrastOf(context);
    final dark = theme.brightness == Brightness.dark;
    final surface = theme.colorScheme.surface
        .withOpacity(highContrast ? 1 : (dark ? 0.84 : 0.90));
    final sheet = DecoratedBox(
      decoration: BoxDecoration(
        color: surface,
        borderRadius: BorderRadius.circular(24),
        border:
            Border.all(color: theme.colorScheme.onSurface.withOpacity(0.12)),
      ),
      child: Column(mainAxisSize: MainAxisSize.min, children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(24, 24, 24, 16),
          child: Align(
            alignment: AlignmentDirectional.centerStart,
            child: Semantics(
              namesRoute: true,
              child: DefaultTextStyle(
                  style: theme.textTheme.headlineSmall!, child: title),
            ),
          ),
        ),
        Flexible(
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 24),
            child: content,
          ),
        ),
        Padding(
          padding: const EdgeInsets.all(16),
          child: OverflowBar(
            alignment: MainAxisAlignment.end,
            spacing: 8,
            overflowSpacing: 8,
            children: actions,
          ),
        ),
      ]),
    );
    return Dialog(
      backgroundColor: Colors.transparent,
      surfaceTintColor: Colors.transparent,
      elevation: 0,
      child: ConstrainedBox(
        constraints: BoxConstraints(maxWidth: maxWidth),
        child: ClipRRect(
          borderRadius: BorderRadius.circular(24),
          child: highContrast
              ? sheet
              : BackdropFilter(
                  filter: ui.ImageFilter.blur(sigmaX: 20, sigmaY: 20),
                  // Replace the sampled backdrop rather than blending another
                  // copy over it inside the route/material compositing layer.
                  blendMode: BlendMode.src,
                  child: sheet,
                ),
        ),
      ),
    );
  }
}
