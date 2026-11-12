import 'package:flutter/material.dart';
import 'package:get/get.dart';

import '../../common.dart';
import 'login.dart';
import '../../models/peer_model.dart';

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
                ...devices.map(_device),
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
