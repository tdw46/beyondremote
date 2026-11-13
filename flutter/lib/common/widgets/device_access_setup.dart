import 'package:flutter/material.dart';
import 'package:window_manager/window_manager.dart';
import 'setup_glass_dialog.dart';

/// OS-local checks only. Completing a prompt is never proof of permission.
class DeviceAccessStep {
  final String title;
  final String description;
  final String action;
  final Future<bool> Function() check;
  final Future<void> Function() request;

  const DeviceAccessStep({
    required this.title,
    required this.description,
    required this.action,
    required this.check,
    required this.request,
  });
}

class DeviceAccessSetup extends StatefulWidget {
  final List<DeviceAccessStep> steps;
  final bool observeWindowFocus;
  const DeviceAccessSetup({
    super.key,
    required this.steps,
    this.observeWindowFocus = true,
  });

  @override
  State<DeviceAccessSetup> createState() => _DeviceAccessSetupState();
}

class _DeviceAccessSetupState extends State<DeviceAccessSetup>
    with WidgetsBindingObserver, WindowListener {
  final _revision = ValueNotifier<int>(0);
  List<bool> _granted = [];
  bool _checking = false;
  bool _requesting = false;
  bool _dialogOpen = false;
  String? _error;
  bool get _ready =>
      _granted.length == widget.steps.length && _granted.every((v) => v);

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    if (widget.observeWindowFocus) windowManager.addListener(this);
    WidgetsBinding.instance.addPostFrameCallback((_) async {
      await _refresh();
      if (mounted && !_ready) _open();
    });
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    if (widget.observeWindowFocus) windowManager.removeListener(this);
    // The dialog can outlive this widget during navigation. Its listener must
    // remain valid until the route closes; no further notifications are sent.
    if (!_dialogOpen) _revision.dispose();
    super.dispose();
  }

  @override
  void onWindowFocus() => _refresh();

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) _refresh();
  }

  void _changed() {
    if (!mounted) return;
    setState(() {});
    _revision.value++;
  }

  Future<void> _refresh() async {
    if (!mounted || _checking) return;
    _checking = true;
    try {
      final values = <bool>[];
      for (final step in widget.steps) {
        values.add(await step.check());
      }
      if (mounted) {
        _granted = values;
        _error = null;
      }
    } catch (_) {
      if (mounted) {
        _granted = [];
        _error = 'Could not check access. Try checking again.';
      }
    } finally {
      _checking = false;
      _changed();
    }
  }

  Future<void> _request(DeviceAccessStep step) async {
    if (_requesting || !mounted) return;
    _requesting = true;
    _error = null;
    _changed();
    try {
      await step.request();
      await _refresh();
    } catch (_) {
      if (mounted) _error = 'Setup did not finish. Please try again.';
    } finally {
      _requesting = false;
      _changed();
    }
  }

  Future<void> _open() async {
    if (!mounted || _dialogOpen) return;
    _dialogOpen = true;
    await showDialog<void>(
      context: context,
      builder: (dialogContext) => ValueListenableBuilder<int>(
        valueListenable: _revision,
        builder: (_, __, ___) => SetupGlassDialog(
          title:
              Text(_ready ? 'Device access is set up' : 'Set up this computer'),
          content: SizedBox(
            width: 440,
            child: SingleChildScrollView(
              child: Column(mainAxisSize: MainAxisSize.min, children: [
                const Text(
                    'Allow your signed-in devices to view and control this computer. Your operating system asks for each permission; you stay in control.'),
                const SizedBox(height: 16),
                for (var i = 0; i < widget.steps.length; i++)
                  _step(dialogContext, widget.steps[i], i),
                if (_error != null) Text(_error!),
                if (!_ready)
                  const Text(
                      'After changing access, return here. If your system asks to restart BeyondRemote, do so, then reconnect.'),
              ]),
            ),
          ),
          actions: [
            if (!_ready)
              TextButton(
                onPressed: _requesting ? null : _refresh,
                child: const Text('Check again'),
              ),
            TextButton(
              onPressed: () => Navigator.of(dialogContext).pop(),
              child: Text(_ready ? 'Done' : 'Later'),
            ),
          ],
        ),
      ),
    );
    _dialogOpen = false;
    if (!mounted) _revision.dispose();
  }

  Widget _step(BuildContext context, DeviceAccessStep step, int index) {
    final granted = index < _granted.length && _granted[index];
    final previousReady =
        _granted.length >= index && _granted.take(index).every((v) => v);
    return Padding(
      padding: const EdgeInsets.only(bottom: 16),
      child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Icon(granted ? Icons.check_circle : Icons.lock_outline,
            color: granted ? Theme.of(context).colorScheme.primary : null),
        const SizedBox(width: 12),
        Expanded(
          child:
              Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text(step.title, style: Theme.of(context).textTheme.titleMedium),
            Text(step.description),
            if (!granted)
              OutlinedButton(
                onPressed: previousReady && !_requesting && mounted
                    ? () => _request(step)
                    : null,
                child: Text(step.action),
              ),
          ]),
        ),
      ]),
    );
  }

  @override
  Widget build(BuildContext context) {
    if (widget.steps.isEmpty) return const SizedBox.shrink();
    return Card(
      child: ListTile(
        leading: Icon(_ready ? Icons.verified_user : Icons.lock_outline),
        title:
            Text(_ready ? 'Device access is set up' : 'Device access needed'),
        subtitle: Text(_ready
            ? 'Local setup checks passed. Verify a connection from your other device.'
            : 'Finish setup so your other devices can view and control this computer.'),
        trailing: TextButton(
          onPressed: _open,
          child: Text(_ready ? 'Review' : 'Finish setup'),
        ),
      ),
    );
  }
}
