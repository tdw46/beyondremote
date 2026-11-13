import 'package:flutter_hbb/models/peer_model.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('signed-in device remains connectable and wakeable after cache restore',
      () {
    final peer = Peer.fromJson({
      'id': 'owned-test-device',
      'platform': 'Windows',
      'connection_protocol': 'ticket-v1',
      'wake_enabled': true,
    });
    for (final restored in [
      Peer.fromJson(peer.toGroupCacheJson()),
      Peer.copy(peer)
    ]) {
      expect(restored.id, peer.id);
      expect(restored.connectionProtocol, 'ticket-v1');
      expect(restored.wakeEnabled, isTrue);
    }
  });
  test('old cache never invents connection capability or wake permission', () {
    final peer = Peer.fromJson({'id': 'legacy-device'});
    expect(peer.connectionProtocol, isEmpty);
    expect(peer.wakeEnabled, isFalse);
  });
}
