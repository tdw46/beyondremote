import 'dart:async';
import 'dart:convert';
import 'package:flutter/material.dart';
import 'package:get/get.dart';

import '../../common.dart';
import 'login.dart';
import 'setup_glass_dialog.dart';
import 'setup_section.dart';
import '../../models/peer_model.dart';
import '../../models/platform_model.dart';

/// Account-owned devices are the only entry point for the new connection flow.
/// No discovery polling: existing login events, explicit refresh and connect
/// intent load authoritative account state.
class AccountDevicesPage extends StatefulWidget {
  final Widget permissions;
  final Widget? setup;
  const AccountDevicesPage({super.key, required this.permissions, this.setup});

  @override
  State<AccountDevicesPage> createState() => _AccountDevicesPageState();
}

class _AccountDevicesPageState extends State<AccountDevicesPage> {
  final _setupRevision = ValueNotifier<int>(0);
  bool _setupDialogOpen = false;
  String? _expandedSetup;
  bool _firewallBusy = false;
  bool _refreshing = false;
  String? _connecting;
  String? _error;
  List<Map<String, dynamic>> _servers = [];
  String? _serverChoice;
  String? _selectedServerId;
  bool _savingServer = false;
  String _serverMessage = '';
  Map<String, dynamic> _serverStatus = {};
  String _myId = '';
  bool _watchingSetup = false;
  bool _wakeEnabled = false;
  String _wakeMessage = '';
  bool _wakeSetupBusy = false;
  bool _wakeBusy = false;
  bool _savingWake = false;
  int _wakeGeneration = 0;
  final _manualHost = TextEditingController();

  @override
  void dispose() {
    _wakeGeneration++;
    _manualHost.dispose();
    if (!_setupDialogOpen) _setupRevision.dispose();
    super.dispose();
  }

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && gFFI.userModel.isLogin) _loadServers();
    });
  }

  Future<void> _loadServers() async {
    try {
      if (isDesktop) _myId = await bind.mainGetMyId();
      final servers = await gFFI.userModel.connectionServers();
      if (!mounted) return;
      setState(() {
        _servers = servers.where((s) => s['desktop'] == true).toList();
        _selectedServerId = null;
        _serverChoice = _servers.any((s) => s['id'] == _myId) ? _myId : null;
        for (final server in _servers) {
          if (server['selected'] == true) {
            _serverChoice = server['id'] as String;
            _selectedServerId = _serverChoice;
          }
        }
      });
      if (isDesktop) {
        _myId = await bind.mainGetMyId();
        await _readWakeSettings();
        await _readServerStatus();
        if (_serverStatus['starting'] == true) _watchSetup();
      }
    } catch (_) {
      if (mounted) {
        setState(() =>
            _error = 'Could not load your connection server. Try Refresh.');
      }
    }
  }

  Future<void> _saveServer() async {
    final choice = _serverChoice;
    if (choice == null || _savingServer) return;
    setState(() {
      _savingServer = true;
      _error = null;
    });
    try {
      await gFFI.userModel.selectConnectionServer(choice);
      await _loadServers();
      if (choice == _myId) await _watchSetup();
    } catch (_) {
      if (mounted) {
        setState(
            () => _error = 'Server selection was not saved. Please retry.');
      }
    } finally {
      if (mounted) {
        setState(() => _savingServer = false);
        _setupRevision.value++;
      }
    }
  }

  Future<void> _readServerStatus() async {
    final status =
        jsonDecode(await bind.mainGetCommon(key: 'managed-server-status'))
            as Map<String, dynamic>;
    if (mounted) {
      setState(() {
        _serverStatus = status;
        _serverMessage = status['message']?.toString() ?? '';
        _firewallBusy = status['firewall_setup']?['busy'] == true;
      });
      _setupRevision.value++;
    }
  }

  Future<void> _watchSetup({bool publish = true}) async {
    if (_watchingSetup || !isDesktop) return;
    _watchingSetup = true;
    try {
      // Local IPC only, bounded to the active wizard. Never an HTTP heartbeat.
      for (var i = 0; i < 90 && mounted; i++) {
        await _readServerStatus();
        if (_serverStatus['starting'] != true &&
            _serverStatus['inspecting'] != true) break;
        await Future<void>.delayed(const Duration(seconds: 1));
      }
      if (publish && mounted && _serverStatus['running'] == true) {
        await gFFI.userModel
            .refreshServerConfigFromAccount(force: true, refreshModels: false);
      }
    } finally {
      _watchingSetup = false;
    }
  }

  void _toggleSetup(String name) {
    setState(() => _expandedSetup = _expandedSetup == name ? null : name);
    _setupRevision.value++;
  }

  Widget _section(
          String id, String title, String summary, IconData icon, Widget child,
          {Color? color}) =>
      SetupSection(
          title: title,
          summary: summary,
          icon: icon,
          statusColor: color,
          expanded: _expandedSetup == id,
          onToggle: () => _toggleSetup(id),
          child: child);

  Widget _fact(String title, String value) => Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
        SizedBox(
            width: 130,
            child: Text(title, style: const TextStyle(color: Colors.grey))),
        Expanded(child: SelectableText(value)),
      ]));

  Future<void> _checkSetup({bool repair = false}) async {
    try {
      await bind.mainSetCommon(
          key: repair ? 'managed-server-refresh' : 'managed-server-inspect',
          value: '');
      await _watchSetup(publish: repair);
    } catch (_) {
      if (mounted) {
        setState(() =>
            _serverMessage = 'Could not finish the local check. Try again.');
        _setupRevision.value++;
      }
    }
  }

  Widget _serverPicker() {
    final ready = _serverStatus['running'] == true;
    final mapped = _serverStatus['router_mapped'] == true;
    final checking = _serverStatus['starting'] == true ||
        _serverStatus['inspecting'] == true;
    final firewall = _serverStatus['firewall_setup'] as Map? ?? {};
    final network = _serverStatus['network'] as Map? ?? {};
    final selected = _selectedServerId == _myId;
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      const Text('We’ll check your network and guide you through what’s left.',
          style: TextStyle(fontSize: 15, color: Colors.grey)),
      const SizedBox(height: 12),
      Container(
          padding: const EdgeInsets.all(12),
          margin: const EdgeInsets.only(bottom: 12),
          decoration: BoxDecoration(
              border: Border.all(color: Colors.grey.withOpacity(0.35)),
              borderRadius: BorderRadius.circular(14)),
          child: Row(children: [
            const Icon(Icons.laptop_outlined, size: 24),
            const SizedBox(width: 14),
            Expanded(
                child: Text(isMacOS ? 'This Mac' : 'This computer',
                    style: const TextStyle(
                        fontSize: 18, fontWeight: FontWeight.w600))),
            Text(selected ? 'Selected home server' : 'Choose a home server',
                style: const TextStyle(color: Colors.grey, fontSize: 14))
          ])),
      _section(
          'computer',
          'Computer & server',
          ready ? 'Server software running' : 'Server software ready',
          ready ? Icons.check_circle_outline : Icons.laptop_outlined,
          Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            if (_servers.isNotEmpty)
              DropdownButtonFormField<String>(
                  value: _serverChoice,
                  isExpanded: true,
                  decoration: const InputDecoration(labelText: 'Home computer'),
                  items: _servers
                      .map((s) => DropdownMenuItem<String>(
                          value: s['id'] as String,
                          child: Text(s['name'].toString(),
                              overflow: TextOverflow.ellipsis)))
                      .toList(),
                  onChanged: _savingServer
                      ? null
                      : (id) {
                          setState(() => _serverChoice = id);
                          _setupRevision.value++;
                        }),
            const SizedBox(height: 12),
            Align(
                alignment: Alignment.centerLeft,
                child: FilledButton(
                    onPressed: _savingServer || _serverChoice == null
                        ? null
                        : _saveServer,
                    child: Text(_savingServer
                        ? 'Setting up…'
                        : _serverChoice == _myId
                            ? 'Set up this computer'
                            : 'Use selected computer'))),
            const SizedBox(height: 8),
            const Text(
                'Keep the selected computer awake with BeyondRemote open. Direct connections are preferred.'),
          ]),
          color: ready ? Colors.greenAccent : null),
      _section(
          'network',
          'Network access',
          checking
              ? 'Checking this computer…'
              : mapped
                  ? 'Router mappings verified'
                  : 'Needs attention',
          Icons.router_outlined,
          Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            _fact('Local address',
                network['local_address']?.toString() ?? 'Checking…'),
            _fact('Router', network['router']?.toString() ?? 'Not detected'),
            if ((_serverStatus['public_host']?.toString() ?? '').isNotEmpty)
              _fact(
                  'Internet address', _serverStatus['public_host'].toString()),
            Container(
                padding: const EdgeInsets.all(12),
                margin: const EdgeInsets.symmetric(vertical: 8),
                decoration: BoxDecoration(
                    color: Colors.grey.withOpacity(0.10),
                    borderRadius: BorderRadius.circular(8),
                    border: Border.all(color: Colors.grey.withOpacity(0.3))),
                child: Text(network['error']?.toString() ??
                    (mapped
                        ? 'Router rules are verified. Test a connection from another network to confirm internet access.'
                        : _serverMessage))),
            if (checking) const LinearProgressIndicator(),
            Wrap(spacing: 12, runSpacing: 8, children: [
              FilledButton(
                  onPressed: checking || !selected
                      ? null
                      : () => _checkSetup(repair: true),
                  child: const Text('Check again')),
              OutlinedButton(
                  onPressed: network['router'] == null
                      ? null
                      : () async {
                          await bind.mainSetCommon(
                              key: 'managed-server-open-router', value: '');
                          await _readServerStatus();
                        },
                  child: const Text('Open router')),
            ]),
          ]),
          color: mapped ? Colors.greenAccent : Colors.amber),
      _section(
          'firewall',
          'Firewall',
          firewall['verified'] == true
              ? (firewall['mode'] == 'off'
                  ? 'Not blocking · firewall is off'
                  : 'App access verified')
              : 'Check access',
          Icons.shield_outlined,
          Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text(firewall['message']?.toString() ??
                'Checking system firewall access.'),
            const SizedBox(height: 12),
            if (_firewallBusy) const LinearProgressIndicator(),
            Wrap(spacing: 12, runSpacing: 8, children: [
              if (firewall['verified'] != true)
                FilledButton(
                    onPressed: _firewallBusy ? null : _prepareFirewall,
                    child: const Text('Allow automatically')),
              OutlinedButton(
                  onPressed: () => _openSystemSettings('firewall'),
                  child: const Text('Open settings')),
            ]),
          ]),
          color: firewall['verified'] == true ? Colors.greenAccent : null),
      _section('wake', 'Wake this computer', 'Optional',
          Icons.nights_stay_outlined, _wakeContent()),
      _section(
          'advanced',
          'Advanced details',
          'Manual setup and recovery',
          Icons.tune,
          Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            const Text(
                'If automatic setup is unavailable, forward TCP 21115–21117, TCP 21128 and UDP 21116 to the detected local address. Double NAT or an ISP-shared address may need upstream setup. Enter router credentials only on your router’s own trusted page.'),
            const SizedBox(height: 12),
            TextField(
                controller: _manualHost,
                decoration: const InputDecoration(
                    labelText: 'Public IPv4 address or DNS name')),
            TextButton(
                onPressed: !selected || checking || ready
                    ? null
                    : () async {
                        await bind.mainSetCommon(
                            key: 'managed-server-manual-start',
                            value: _manualHost.text.trim());
                        await _watchSetup();
                      },
                child: const Text('Use manual setup')),
            if (ready || checking)
              TextButton(
                  onPressed: () async {
                    await bind.mainSetCommon(
                        key: 'managed-server-stop', value: '');
                    await _readServerStatus();
                  },
                  child: const Text('Stop server and remove its mappings')),
          ])),
    ]);
  }

  Widget _wakeContent() =>
      Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        const Text(
            'Keep another signed-in computer awake on the same home network to send the wake signal.'),
        const SizedBox(height: 12),
        FilledButton(
            onPressed: _wakeSetupBusy ? null : _prepareWake,
            child: Text(_wakeSetupBusy
                ? 'Waiting for system approval…'
                : 'Enable automatically')),
        if (_wakeSetupBusy) const LinearProgressIndicator(),
        const SizedBox(height: 12),
        SelectableText(_wakeMessage),
        ExpansionTile(title: const Text('Settings & sleep test'), children: [
          Wrap(spacing: 8, children: [
            TextButton(
                onPressed: () => _openSystemSettings('power'),
                child: const Text('Power settings')),
            if (isMacOS)
              TextButton(
                  onPressed: () => _openSystemSettings('energy'),
                  child: const Text('Desktop Energy settings')),
            if (isWindows)
              TextButton(
                  onPressed: () => _openSystemSettings('adapters'),
                  child: const Text('Device Manager')),
            TextButton(
                onPressed: () => _openSystemSettings('network'),
                child: const Text('Network settings')),
          ]),
          Text(isMacOS
              ? 'Enable Wake for network access in Battery Options or desktop Energy settings. AC power only; battery policy stays unchanged.'
              : 'In Network adapters, check Wake on Magic Packet and Allow this device to wake the computer. Missing controls can mean the driver or hardware does not support it.'),
          const SizedBox(height: 8),
          const Text(
              'Put this computer to sleep, then choose Wake from another signed-in device. Applied settings alone do not prove wake works.'),
        ]),
      ]);

  Future<void> _openSystemSettings(String target) async {
    await bind.mainSetCommon(key: 'setup-open-settings', value: target);
  }

  Future<void> _prepareFirewall() async {
    if (_firewallBusy) return;
    setState(() => _firewallBusy = true);
    _setupRevision.value++;
    try {
      await bind.mainSetCommon(
          key: 'managed-server-prepare-firewall', value: '');
      for (var i = 0; i < 125 && mounted; i++) {
        await _readServerStatus();
        if (!_firewallBusy) break;
        await Future<void>.delayed(const Duration(seconds: 1));
      }
    } catch (_) {
      if (mounted) {
        setState(() => _serverMessage =
            'Could not read firewall setup. Open system firewall settings and review app access.');
      }
    } finally {
      if (mounted) {
        setState(() => _firewallBusy = false);
        _setupRevision.value++;
      }
    }
  }

  Future<void> _prepareWake() async {
    if (_wakeSetupBusy) return;
    setState(() => _wakeSetupBusy = true);
    _setupRevision.value++;
    try {
      await bind.mainSetCommon(key: 'wake-prepare', value: '');
      for (var i = 0; i < 125 && mounted; i++) {
        await _readWakeSettings();
        if (!_wakeSetupBusy) break;
        await Future<void>.delayed(const Duration(seconds: 1));
      }
      if (mounted && _wakeEnabled) {
        await gFFI.userModel
            .refreshServerConfigFromAccount(force: true, refreshModels: false);
      }
    } catch (_) {
      if (mounted) {
        setState(() => _wakeMessage =
            'Could not read the wake setup result. No success is confirmed. Open system settings or retry.');
      }
    } finally {
      if (mounted) {
        setState(() => _wakeSetupBusy = false);
        _setupRevision.value++;
      }
    }
  }

  Future<void> _showSetupDialog({required bool wake}) async {
    if (_setupDialogOpen) return;
    _setupDialogOpen = true;
    _expandedSetup =
        !wake && _serverStatus['router_mapped'] != true ? 'network' : null;
    if (wake) {
      unawaited(_readWakeSettings());
    } else {
      unawaited(_checkSetup());
    }
    if (!mounted) {
      _setupDialogOpen = false;
      _setupRevision.dispose();
      return;
    }
    await showDialog<void>(
        context: context,
        builder: (dialogContext) => ValueListenableBuilder<int>(
            valueListenable: _setupRevision,
            builder: (_, __, ___) => SetupGlassDialog(
                  maxWidth: 680,
                  title: Row(children: [
                    Icon(wake ? Icons.nights_stay_outlined : Icons.laptop_mac,
                        color: const Color(0xff2196ef), size: 32),
                    const SizedBox(width: 16),
                    Expanded(
                        child: Text(
                            wake
                                ? 'Wake this computer'
                                : 'Set up this computer',
                            style: const TextStyle(
                                fontSize: 24, fontWeight: FontWeight.w600)))
                  ]),
                  content: SizedBox(
                      width: 620,
                      child: SingleChildScrollView(
                          child: wake ? _wakeContent() : _serverPicker())),
                  actions: [
                    if (!wake)
                      const Text(
                          'Only BeyondRemote traffic uses this connection.',
                          style: TextStyle(fontSize: 12, color: Colors.grey)),
                    FilledButton(
                        onPressed: () => Navigator.of(dialogContext).pop(),
                        child: const Text('Done'))
                  ],
                )));
    _setupDialogOpen = false;
    if (!mounted) _setupRevision.dispose();
  }

  Future<void> _readWakeSettings() async {
    final value = jsonDecode(await bind.mainGetCommon(key: 'wake-settings'))
        as Map<String, dynamic>;
    if (mounted) {
      setState(() {
        _wakeEnabled = value['enabled'] == true;
        _wakeSetupBusy = value['setup']?['busy'] == true;
        final setupMessage = value['setup']?['message']?.toString() ?? '';
        _wakeMessage = setupMessage.isNotEmpty
            ? setupMessage
            : value['message']?.toString() ?? '';
        if (_wakeEnabled && value['profiles'] == 0) {
          _wakeMessage +=
              '\nNo supported local network adapter found. Connect to your home network and try again.';
        }
      });
      _setupRevision.value++;
    }
  }

  Future<void> _setWakeEnabled(bool enabled) async {
    setState(() => _savingWake = true);
    try {
      await bind.mainSetCommon(key: 'wake-enabled', value: enabled ? 'Y' : '');
      await _readWakeSettings();
      await gFFI.userModel
          .refreshServerConfigFromAccount(force: true, refreshModels: false);
    } catch (_) {
      if (mounted) {
        setState(() => _error = 'Could not save wake settings. Please retry.');
      }
    } finally {
      if (mounted) setState(() => _savingWake = false);
    }
  }

  Future<void> _wakeAndConnect(Peer peer) async {
    if (_wakeBusy || _connecting != null) return;
    final generation = ++_wakeGeneration;
    setState(() {
      _wakeBusy = true;
      _error = null;
    });
    try {
      await bind.mainSetCommon(key: 'wake-device', value: peer.id);
      // Local status only, bounded to this explicit request. No hosted polling.
      for (var i = 0; i < 36 && mounted && generation == _wakeGeneration; i++) {
        final status = jsonDecode(await bind.mainGetCommon(key: 'wake-status'))
            as Map<String, dynamic>;
        if (status['target'] == peer.id && status['state'] == 'error') {
          setState(
              () => _error = status['message']?.toString() ?? 'Wake failed.');
          return;
        }
        if (status['target'] == peer.id && status['state'] == 'sent') {
          setState(() =>
              _error = 'Wake signal sent. Waiting briefly before connecting…');
          await Future<void>.delayed(const Duration(seconds: 8));
          if (mounted &&
              generation == _wakeGeneration &&
              gFFI.userModel.isLogin) {
            await _connect(peer);
          }
          return;
        }
        await Future<void>.delayed(const Duration(seconds: 1));
      }
      if (mounted && generation == _wakeGeneration) {
        setState(() => _error = 'Wake timed out. Check your home computer.');
      }
    } finally {
      if (mounted && generation == _wakeGeneration) {
        setState(() => _wakeBusy = false);
      }
    }
  }

  Future<void> _refresh() async {
    if (_refreshing) return;
    setState(() {
      _refreshing = true;
      _error = null;
    });
    try {
      await gFFI.userModel
          .refreshServerConfigFromAccount(force: true, refreshModels: false);
      await gFFI.groupModel.pull(force: true);
      await _loadServers();
    } catch (_) {
      if (mounted) {
        setState(() {
          _error = 'Could not refresh your devices. Please try again.';
        });
      }
    } finally {
      if (mounted) {
        setState(() {
          _refreshing = false;
        });
      }
    }
  }

  Future<void> _connect(Peer peer) async {
    if (_connecting != null || peer.connectionProtocol != 'ticket-v1') return;
    setState(() {
      _connecting = peer.id;
      _error = null;
    });
    try {
      // IDs remain an internal protocol detail, never typed by the user.
      await connect(context, peer.id);
    } catch (_) {
      if (mounted) {
        setState(() {
          _error =
              'Could not connect. Check that both devices are signed in and try again.';
        });
      }
    } finally {
      if (mounted) {
        setState(() {
          _connecting = null;
        });
      }
    }
  }

  Widget _device(Peer peer) {
    final label = peer.alias.isNotEmpty
        ? peer.alias
        : peer.hostname.isNotEmpty
            ? peer.hostname
            : 'Your device';
    final busy = _connecting == peer.id;
    final compatible = peer.connectionProtocol == 'ticket-v1';
    return Card(
      margin: const EdgeInsets.only(bottom: 12),
      child: Padding(
        padding: const EdgeInsets.all(20),
        child: Row(children: [
          const Icon(Icons.computer_rounded, size: 28),
          const SizedBox(width: 16),
          Expanded(
              child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                Text(label, style: Theme.of(context).textTheme.titleMedium),
                const SizedBox(height: 4),
                Text(peer.platform.isEmpty
                    ? 'Signed in to your account'
                    : peer.platform),
                if (!compatible)
                  const Text('Update BeyondRemote on this device to connect.'),
              ])),
          const SizedBox(width: 12),
          if (isDesktop && peer.wakeEnabled)
            IconButton(
              tooltip: 'Wake and connect',
              onPressed: _connecting != null || _wakeBusy
                  ? null
                  : () => _wakeAndConnect(peer),
              icon: const Icon(Icons.power_settings_new_rounded),
            ),
          FilledButton.icon(
            onPressed: _connecting != null || !compatible
                ? null
                : () => _connect(peer),
            icon: busy
                ? const SizedBox(
                    width: 16,
                    height: 16,
                    child: CircularProgressIndicator(strokeWidth: 2))
                : const Icon(Icons.arrow_forward_rounded, size: 18),
            label: Text(busy ? 'Connecting…' : 'Connect'),
          ),
        ]),
      ),
    );
  }

  @override
  Widget build(BuildContext context) => Obx(() {
        final user = gFFI.userModel;
        final signedIn = user.userName.value.isNotEmpty;
        final devices = gFFI.groupModel.peers.toList();
        final loading = gFFI.groupModel.groupLoading.value || _refreshing;
        final loadFailed = gFFI.groupModel.groupLoadError.value.isNotEmpty ||
            user.networkError.value.isNotEmpty;
        return SingleChildScrollView(
          padding:
              EdgeInsets.all(MediaQuery.sizeOf(context).width < 600 ? 16 : 28),
          child: Center(
              child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 850),
            child:
                Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text(signedIn ? 'Your devices' : 'Your devices, together.',
                  style: Theme.of(context).textTheme.headlineMedium),
              const SizedBox(height: 10),
              Text(signedIn
                  ? user.accountLabelWithHandle
                  : 'Sign in to BeyondRemote on each device. Then choose a device to connect.'),
              const SizedBox(height: 24),
              if (widget.setup != null) widget.setup!,
              const SizedBox(height: 16),
              if (!signedIn)
                FilledButton.icon(
                    onPressed: () async {
                      await loginDialog();
                      if (mounted && user.isLogin) await _refresh();
                    },
                    icon: const Icon(Icons.person_outline_rounded),
                    label: const Text('Sign in')),
              if (signedIn) ...[
                Row(children: [
                  const Expanded(
                      child: Text(
                          'Only devices belonging to your account appear here.')),
                  IconButton(
                      tooltip: 'Refresh devices',
                      onPressed: loading ? null : _refresh,
                      icon: const Icon(Icons.refresh_rounded)),
                ]),
                const SizedBox(height: 16),
                if (loading) const LinearProgressIndicator(),
                if (_error != null || loadFailed)
                  Padding(
                      padding: const EdgeInsets.symmetric(vertical: 12),
                      child: Text(_error ??
                          'Your device list could not be refreshed. Try again when you are connected.')),
                if (!loading && devices.isEmpty && !loadFailed)
                  const Padding(
                      padding: EdgeInsets.symmetric(vertical: 28),
                      child: Text(
                          'No other devices yet. Sign in to the same account on your other computer to add it here.')),
                if (_wakeBusy)
                  Row(children: [
                    const Expanded(child: Text('Waking your computer…')),
                    TextButton(
                        onPressed: () {
                          _wakeGeneration++;
                          setState(() => _wakeBusy = false);
                        },
                        child: const Text('Stop waiting')),
                  ]),
                ...devices.map(_device),
                Card(
                  child: ExpansionTile(
                    leading: const Icon(Icons.shield_outlined),
                    title: const Text('Private network & wake'),
                    subtitle: const Text(
                        'Only BeyondRemote traffic. Your internet stays unchanged.'),
                    children: [
                      Padding(
                        padding: const EdgeInsets.all(16),
                        child: FilledButton.icon(
                          icon: const Icon(Icons.settings_ethernet),
                          label: const Text('Guided server setup'),
                          onPressed: () => _showSetupDialog(wake: false),
                        ),
                      ),
                      if (isDesktop) ...[
                        Padding(
                          padding: const EdgeInsets.symmetric(horizontal: 16),
                          child: OutlinedButton.icon(
                            icon: const Icon(Icons.power_settings_new_rounded),
                            label: const Text('Set up wake'),
                            onPressed: () => _showSetupDialog(wake: true),
                          ),
                        ),
                        SwitchListTile(
                          title: const Text(
                              'Allow my devices to wake this computer'),
                          subtitle: const Text(
                              'Requires supported hardware and another awake computer at home.'),
                          value: _wakeEnabled,
                          onChanged: _savingWake || _wakeSetupBusy
                              ? null
                              : _setWakeEnabled,
                        ),
                        Padding(
                            padding: const EdgeInsets.all(16),
                            child: Text(_wakeMessage)),
                      ],
                    ],
                  ),
                ),
                const SizedBox(height: 16),
                const Text(
                    'Connections choose the best available path automatically.'),
                const SizedBox(height: 20),
                widget.permissions,
              ],
            ]),
          )),
        );
      });
}
