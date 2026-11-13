import 'package:flutter/material.dart';

/// Compact, keyboard-accessible disclosure shared by all first-run setup steps.
class SetupSection extends StatelessWidget {
  final String title;
  final String summary;
  final IconData icon;
  final Color? statusColor;
  final bool expanded;
  final VoidCallback onToggle;
  final Widget child;
  const SetupSection(
      {super.key,
      required this.title,
      required this.summary,
      required this.icon,
      this.statusColor,
      required this.expanded,
      required this.onToggle,
      required this.child});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final color = statusColor ?? theme.colorScheme.onSurface.withOpacity(0.76);
    return Container(
      margin: const EdgeInsets.only(bottom: 10),
      decoration: BoxDecoration(
        color: theme.colorScheme.onSurface.withOpacity(0.025),
        border:
            Border.all(color: theme.colorScheme.onSurface.withOpacity(0.22)),
        borderRadius: BorderRadius.circular(16),
      ),
      child: Column(mainAxisSize: MainAxisSize.min, children: [
        Semantics(
          button: true,
          expanded: expanded,
          child: InkWell(
              borderRadius: BorderRadius.circular(16),
              onTap: onToggle,
              child: Padding(
                  padding:
                      const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
                  child: Row(children: [
                    Icon(icon, size: 24, color: color),
                    const SizedBox(width: 16),
                    Expanded(
                        child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                          Text(title,
                              style: const TextStyle(
                                  fontSize: 16, fontWeight: FontWeight.w600)),
                          const SizedBox(height: 2),
                          Text(summary,
                              style: TextStyle(fontSize: 12, color: color)),
                        ])),
                    Icon(
                        expanded
                            ? Icons.keyboard_arrow_up
                            : Icons.keyboard_arrow_down,
                        size: 24),
                  ]))),
        ),
        AnimatedSize(
            duration: MediaQuery.disableAnimationsOf(context)
                ? Duration.zero
                : const Duration(milliseconds: 180),
            alignment: Alignment.topCenter,
            child: expanded
                ? Padding(
                    padding: const EdgeInsets.fromLTRB(20, 2, 20, 18),
                    child: child)
                : const SizedBox.shrink()),
      ]),
    );
  }
}
