import 'package:flutter/material.dart';
import 'package:flutter_hbb/common/widgets/device_access_setup.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('denial stays pending; resume advances; revocation is visible',
      (tester) async {
    var screen = false;
    var input = false;
    var prompts = 0;
    await tester.pumpWidget(MaterialApp(
        home: Scaffold(
            body: DeviceAccessSetup(
      observeWindowFocus: false,
      steps: [
        DeviceAccessStep(
            title: 'Screen',
            description: 'Screen access',
            action: 'Allow screen',
            check: () async => screen,
            request: () async {
              prompts++;
            }),
        DeviceAccessStep(
            title: 'Input',
            description: 'Input access',
            action: 'Allow input',
            check: () async => input,
            request: () async {
              input = true;
            }),
      ],
    ))));
    await tester.pumpAndSettle();
    expect(find.text('Set up this computer'), findsOneWidget);
    expect(prompts, 0);
    expect(find.text('Allow input'), findsNothing);
    expect(find.text('Input'), findsOneWidget);
    await tester.tap(find.text('Allow screen'));
    await tester.pumpAndSettle();
    expect(prompts, 1);
    expect(find.text('Allow screen'), findsOneWidget);
    screen = true;
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
    await tester.pumpAndSettle();
    expect(find.text('Allow screen'), findsNothing);
    await tester.tap(find.text('Allow input'));
    await tester.pumpAndSettle();
    expect(find.text('Done'), findsOneWidget);
    await tester.tap(find.text('Done'));
    await tester.pumpAndSettle();
    expect(find.byType(AlertDialog), findsNothing);
    input = false;
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
    await tester.pumpAndSettle();
    expect(find.text('Device access needed'), findsOneWidget);
    expect(find.byType(AlertDialog), findsNothing);
    await tester.tap(find.text('Finish setup'));
    await tester.pumpAndSettle();
    expect(find.text('Allow input'), findsOneWidget);
  });
  testWidgets('Windows cancelled installation never reports ready',
      (tester) async {
    var checks = 0;
    await tester.pumpWidget(MaterialApp(
        home: Scaffold(
            body: DeviceAccessSetup(
      observeWindowFocus: false,
      steps: [
        DeviceAccessStep(
            title: 'Windows access',
            description: 'Approve installation',
            action: 'Install',
            check: () async {
              checks++;
              return false;
            },
            request: () async {})
      ],
    ))));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Install'));
    await tester.pumpAndSettle();
    expect(checks, 2);
    expect(find.text('Done'), findsNothing);
    await tester.tap(find.text('Later'));
    await tester.pumpAndSettle();
    expect(find.text('Finish setup'), findsOneWidget);
    expect(find.byType(AlertDialog), findsNothing);
  });
  testWidgets('configured devices do not open setup', (tester) async {
    await tester.pumpWidget(MaterialApp(
        home: Scaffold(
            body: DeviceAccessSetup(
      observeWindowFocus: false,
      steps: [
        DeviceAccessStep(
            title: 'Ready',
            description: '',
            action: 'Allow',
            check: () async => true,
            request: () async {
              throw StateError('must not prompt');
            })
      ],
    ))));
    await tester.pumpAndSettle();
    expect(find.byType(AlertDialog), findsNothing);
    expect(find.text('Device access is set up'), findsOneWidget);
  });
}
