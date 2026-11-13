import 'dart:convert';
import 'package:flutter/material.dart';
import 'package:get/get.dart';

import '../../common.dart';
import 'login.dart';
import '../../models/peer_model.dart';
import '../../models/platform_model.dart';

/// Account-owned devices are the only entry point for the new connection flow.
/// No discovery polling: existing login events, explicit refresh and connect
/// intent load authoritative account state.
class AccountDevicesPage extends StatefulWidget {
  final Widget permissions;
  const AccountDevicesPage({super.key, required this.permissions});

  @override
  State<AccountDevicesPage> createState() => _AccountDevicesPageState();
}

class _AccountDevicesPageState extends State<AccountDevicesPage> {
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
      if (mounted) setState(() => _savingServer = false);
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
      });
    }
  }

  Future<void> _watchSetup() async {
    if (_watchingSetup || !isDesktop) return;
    _watchingSetup = true;
    try {
      // Local IPC only, bounded to the active wizard. Never an HTTP heartbeat.
      for (var i = 0; i < 90 && mounted; i++) {
        await _readServerStatus();
        if (_serverStatus['starting'] != true) break;
        await Future<void>.delayed(const Duration(seconds: 1));
      }
      if (mounted && _serverStatus['running'] == true) {
        await gFFI.userModel
            .refreshServerConfigFromAccount(force: true, refreshModels: false);
      }
    } finally {
      _watchingSetup = false;
    }
  }

  Widget _setupStep(String label, bool complete) => Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(children: [
        Icon(
            complete
                ? Icons.check_circle_outline
                : Icons.radio_button_unchecked,
            size: 19,
            color: complete ? Theme.of(context).colorScheme.primary : null),
        const SizedBox(width: 8),
        Expanded(child: Text(label)),
      ]));

  Widget _setupWizard() => ExpansionTile(
        initiallyExpanded: false,
        title: const Text('Server setup on this computer'),
        leading: const Icon(Icons.settings_ethernet_rounded),
        childrenPadding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
        children: [
          _setupStep(
              'Computer selected for your account', _selectedServerId == _myId),
          _setupStep(
              'Server software included', _serverStatus['installed'] == true),
          _setupStep('Router permissions and port mappings checked',
              _serverStatus['router_mapped'] == true),
          _setupStep('Local connection server running',
              _serverStatus['running'] == true),
          _setupStep(
              'Internet connection: test from a different network', false),
          if (_serverStatus['starting'] == true)
            const LinearProgressIndicator(),
          const SizedBox(height: 8),
          Text(_serverMessage),
          const SizedBox(height: 12),
          const Text(
              'Automatic setup uses UPnP with your router’s existing permission. If it is unavailable, use your router’s app or local administration page to enable UPnP or forward TCP 21115–21117, TCP 21128 and UDP 21116 to this computer. Approve the server in your operating system’s firewall. A second router or ISP-shared address may require upstream setup.'),
          const SizedBox(height: 8),
          const Text(
              'Enter a router password only in the router’s own trusted sign-in page. BeyondRemote does not upload or store it.'),
          ExpansionTile(title: const Text('Manual router setup'), children: [
            TextField(
                controller: _manualHost,
                decoration: const InputDecoration(
                    labelText: 'Public IPv4 address or DNS name',
                    helperText:
                        'Use after forwarding the listed ports to this computer.')),
            TextButton(
                onPressed: _selectedServerId != _myId ||
                        _serverStatus['starting'] == true ||
                        _serverStatus['running'] == true
                    ? null
                    : () async {
                        await bind.mainSetCommon(
                            key: 'managed-server-manual-start',
                            value: _manualHost.text.trim());
                        await _watchSetup();
                      },
                child: const Text('Use manual router setup')),
          ]),
          const SizedBox(height: 12),
          Wrap(spacing: 8, children: [
            TextButton.icon(
                onPressed: _selectedServerId != _myId ||
                        _serverStatus['starting'] == true
                    ? null
                    : () async {
                        await bind.mainSetCommon(
                            key: 'managed-server-refresh', value: '');
                        await _watchSetup();
                      },
                icon: const Icon(Icons.refresh_rounded),
                label: const Text('Retry automatic setup')),
            if (_serverStatus['starting'] == true ||
                _serverStatus['running'] == true)
              TextButton(
                  onPressed: () async {
                    await bind.mainSetCommon(
                        key: 'managed-server-stop', value: '');
                    await _readServerStatus();
                  },
                  child: const Text('Stop server and remove its mappings')),
          ]),
        ],
      );

  Widget _serverPicker() => Card(
        child: Padding(
            padding: const EdgeInsets.all(20),
            child:
                Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text('Home computer',
                  style: Theme.of(context).textTheme.titleMedium),
              const SizedBox(height: 8),
              const Text(
                  'Choose an account computer to help your devices connect. Keep it awake with BeyondRemote open. Direct connections are preferred.'),
              const SizedBox(height: 12),
              if (_servers.isNotEmpty)
                DropdownButtonFormField<String>(
                  value: _serverChoice,
                  decoration: const InputDecoration(labelText: 'Home computer'),
                  isExpanded: true,
                  items: _servers
                      .map((s) => DropdownMenuItem<String>(
                          value: s['id'] as String,
                          child: Text(s['name'].toString(),
                              overflow: TextOverflow.ellipsis)))
                      .toList(),
                  onChanged: _savingServer
                      ? null
                      : (id) => setState(() => _serverChoice = id),
                ),
              const SizedBox(height: 12),
              FilledButton.icon(
                  onPressed: _savingServer || _serverChoice == null
                      ? null
                      : _saveServer,
                  icon: const Icon(Icons.dns_outlined),
                  label: Text(_savingServer
                      ? 'Setting up…'
                      : _serverChoice == _myId
                          ? 'Set up this computer'
                          : 'Use selected computer')),
              const SizedBox(height: 8),
              const Text(
                  'Changes reach other devices when they refresh or reconnect. A saved selection does not guarantee internet reachability.'),
              if (isDesktop) _setupWizard(),
            ])),
      );

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
          _wakeMessage =
              'No supported local network adapter found. Connect to your home network and try again.';
        }
      });
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
                      _serverPicker(),
                      if (isDesktop) ...[
                        Padding(
                          padding: const EdgeInsets.symmetric(horizontal: 16),
                          child: OutlinedButton.icon(
                            icon: const Icon(Icons.power_settings_new_rounded),
                            label: Text(_wakeSetupBusy
                                ? 'Preparing…'
                                : 'Prepare this computer for wake'),
                            onPressed: _wakeSetupBusy
                                ? null
                                : () async {
                                    await bind.mainSetCommon(
                                        key: 'wake-prepare', value: '');
                                    for (var i = 0; i < 125 && mounted; i++) {
                                      await _readWakeSettings();
                                      if (!_wakeSetupBusy) break;
                                      await Future<void>.delayed(
                                          const Duration(seconds: 1));
                                    }
                                    if (mounted) await _refresh();
                                  },
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
