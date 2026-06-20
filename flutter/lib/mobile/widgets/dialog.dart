import 'dart:async';
import 'dart:convert';
import 'package:flutter/material.dart';
import 'package:flutter_hbb/common/widgets/setting_widgets.dart';
import 'package:flutter_hbb/common/widgets/toolbar.dart';
import 'package:get/get.dart';

import '../../common.dart';
import '../../models/platform_model.dart';

void _showSuccess() {
  showToast(translate("Successful"));
}

void setTemporaryPasswordLengthDialog(
    OverlayDialogManager dialogManager) async {
  List<String> lengths = ['6', '8', '10'];
  String length = await bind.mainGetOption(key: "temporary-password-length");
  var index = lengths.indexOf(length);
  if (index < 0) index = 0;
  length = lengths[index];
  dialogManager.show((setState, close, context) {
    setLength(newValue) {
      final oldValue = length;
      if (oldValue == newValue) return;
      setState(() {
        length = newValue;
      });
      bind.mainSetOption(key: "temporary-password-length", value: newValue);
      bind.mainUpdateTemporaryPassword();
      Future.delayed(Duration(milliseconds: 200), () {
        close();
        _showSuccess();
      });
    }

    return CustomAlertDialog(
      title: Text(translate("Set one-time password length")),
      content: Row(
          mainAxisAlignment: MainAxisAlignment.spaceEvenly,
          children: lengths
              .map(
                (value) => Row(
                  children: [
                    Text(value),
                    Radio(
                        value: value, groupValue: length, onChanged: setLength),
                  ],
                ),
              )
              .toList()),
    );
  }, backDismiss: true, clickMaskDismiss: true);
}

void showServerSettings(OverlayDialogManager dialogManager,
    void Function(VoidCallback) setState) async {
  Map<String, dynamic> options = {};
  try {
    options = jsonDecode(await bind.mainGetOptions());
  } catch (e) {
    print("Invalid server config: $e");
  }
  showServerSettingsWithValue(
      ServerConfig.fromOptions(options), dialogManager, setState);
}

void showServerSettingsWithValue(
    ServerConfig serverConfig,
    OverlayDialogManager dialogManager,
    void Function(VoidCallback)? upSetState) async {
  var isInProgress = false;
  final idCtrl = TextEditingController(text: serverConfig.idServer);
  final relayCtrl = TextEditingController(text: serverConfig.relayServer);
  final apiCtrl = TextEditingController(text: serverConfig.apiServer);
  final keyCtrl = TextEditingController(text: serverConfig.key);
  final managedHbbsCtrl = TextEditingController();
  final managedHbbrCtrl = TextEditingController();
  final managedPublicHostCtrl = TextEditingController();

  RxString idServerMsg = ''.obs;
  RxString relayServerMsg = ''.obs;
  RxString apiServerMsg = ''.obs;
  Map<String, dynamic> managedStatus = {};
  var managedStatusRequested = false;

  final controllers = [idCtrl, relayCtrl, apiCtrl, keyCtrl];
  final errMsgs = [
    idServerMsg,
    relayServerMsg,
    apiServerMsg,
  ];

  dialogManager.show((setState, close, context) {
    Future<void> refreshManagedStatus() async {
      try {
        final status = jsonDecode(
            await bind.mainGetCommon(key: 'managed-server-status'));
        if (status is Map<String, dynamic>) {
          setState(() {
            managedStatus = status;
            if (managedHbbsCtrl.text.isEmpty) {
              managedHbbsCtrl.text = status['hbbs_path']?.toString() ?? '';
            }
            if (managedHbbrCtrl.text.isEmpty) {
              managedHbbrCtrl.text = status['hbbr_path']?.toString() ?? '';
            }
            if (managedPublicHostCtrl.text.isEmpty) {
              managedPublicHostCtrl.text =
                  status['public_host']?.toString() ?? '';
            }
            if (status['running'] == true) {
              idCtrl.text = status['id_server']?.toString() ?? idCtrl.text;
              relayCtrl.text =
                  status['relay_server']?.toString() ?? relayCtrl.text;
              apiCtrl.text = status['api_server']?.toString() ?? apiCtrl.text;
              final key = status['key']?.toString() ?? '';
              if (key.isNotEmpty) {
                keyCtrl.text = key;
              }
            }
          });
        }
      } catch (e) {
        debugPrint('Failed to load managed server status: $e');
      }
    }

    Future<void> refreshManagedStatusFor(Duration duration) async {
      final end = DateTime.now().add(duration);
      do {
        await refreshManagedStatus();
        if (managedStatus['installing'] != true) {
          break;
        }
        await Future.delayed(Duration(seconds: 1));
      } while (DateTime.now().isBefore(end));
      await refreshManagedStatus();
    }

    Widget managedServerPanel() {
      if (isIOS || isAndroid || isWeb) {
        return Offstage();
      }
      if (!managedStatusRequested) {
        managedStatusRequested = true;
        Future.microtask(refreshManagedStatus);
      }
      final supportedInstall = managedStatus['supported_install'] == true;
      final installed = managedStatus['installed'] == true;
      final running = managedStatus['running'] == true;
      final installing = managedStatus['installing'] == true;
      final enabled = managedStatus['enabled'] == true;
      final message = managedStatus['message']?.toString() ??
          'Manage a local open-source hbbs/hbbr server for this client.';
      const publicAccessHelp =
          'Local-only keeps the server on this computer and is best for testing. Home router uses your public home IP or DNS name plus router port forwarding. Forward TCP 21114 for login and address book sync, TCP 21115-21119 for ID/relay services, and UDP 21116 for direct NAT help. Public VPS uses a tiny internet VM. Vercel-style web hosts are not suitable for the relay because this server needs always-on TCP and UDP ports.';

      return Container(
        width: double.infinity,
        padding: EdgeInsets.all(12),
        decoration: BoxDecoration(
          border: Border.all(color: Theme.of(context).dividerColor),
          borderRadius: BorderRadius.circular(6),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Managed self-hosted server',
                style: Theme.of(context).textTheme.titleSmall),
            SizedBox(height: 6),
            Text(message, style: Theme.of(context).textTheme.bodySmall),
            SizedBox(height: 6),
            Tooltip(
              message: publicAccessHelp,
              child: Text(
                'Internet access works best from a public VPS. Home hosting also works when your router forwards TCP 21114-21119 and UDP 21116. Leave the public address empty for local-only testing.',
                style: Theme.of(context).textTheme.bodySmall,
              ),
            ),
            SizedBox(height: 10),
            serverSettingsTextFormField(
              label: 'Public address',
              controller: managedPublicHostCtrl,
              errorMsg: '',
              hintText: 'remote.example.com or 203.0.113.10',
              helperText:
                  'Use a public VPS DNS/IP for easiest internet access. Other devices use this for ID/relay; account sync uses the same host on TCP 21114.',
            ),
            if (!supportedInstall) ...[
              SizedBox(height: 10),
              serverSettingsTextFormField(
                label: 'hbbs path',
                controller: managedHbbsCtrl,
                errorMsg: '',
                helperText: 'Path to a local hbbs binary.',
              ),
              SizedBox(height: 8),
              serverSettingsTextFormField(
                label: 'hbbr path',
                controller: managedHbbrCtrl,
                errorMsg: '',
                helperText: 'Path to a local hbbr binary.',
              ),
            ],
            SizedBox(height: 8),
            Row(
              children: [
                Expanded(
                  child: CheckboxListTile(
                    dense: true,
                    contentPadding: EdgeInsets.zero,
                    title: Text('Start with Beyond Remote'),
                    value: enabled,
                    onChanged: installing
                        ? null
                        : (value) async {
                            await bind.mainSetCommon(
                                key: 'managed-server-enable',
                                value: value == true ? 'Y' : '');
                            await refreshManagedStatusFor(
                                Duration(seconds: 3));
                          },
                  ),
                ),
                if (supportedInstall && !installed)
                  dialogButton(
                    installing ? 'Installing...' : 'Install',
                    onPressed: installing
                        ? null
                        : () async {
                            await bind.mainSetCommon(
                                key: 'managed-server-install',
                                value: managedPublicHostCtrl.text.trim());
                            await refreshManagedStatusFor(
                                Duration(seconds: 90));
                          },
                  )
                else if (running)
                  dialogButton(
                    'Stop',
                    onPressed: installing
                        ? null
                        : () async {
                            await bind.mainSetCommon(
                                key: 'managed-server-stop', value: '');
                            await refreshManagedStatus();
                          },
                    isOutline: true,
                  )
                else
                  dialogButton(
                    'Start',
                    onPressed: installing
                        ? null
                        : () async {
                            if (!supportedInstall) {
                              await bind.mainSetCommon(
                                  key: 'managed-server-set-paths',
                                  value: jsonEncode({
                                    'hbbs': managedHbbsCtrl.text.trim(),
                                    'hbbr': managedHbbrCtrl.text.trim(),
                                  }));
                            }
                            await bind.mainSetCommon(
                                key: 'managed-server-start',
                                value: managedPublicHostCtrl.text.trim());
                            await refreshManagedStatusFor(Duration(seconds: 8));
                          },
                  ),
              ],
            ),
            if (installing)
              LinearProgressIndicator().marginOnly(top: 8),
          ],
        ),
      );
    }

    Future<bool> submit() async {
      setState(() {
        isInProgress = true;
      });
      await bind.mainSetCommon(
          key: 'managed-server-set-public-host',
          value: managedPublicHostCtrl.text.trim());
      bool ret = await setServerConfig(
          null,
          errMsgs,
          ServerConfig(
              idServer: idCtrl.text.trim(),
              relayServer: relayCtrl.text.trim(),
              apiServer: apiCtrl.text.trim(),
              key: keyCtrl.text.trim()));
      setState(() {
        isInProgress = false;
      });
      return ret;
    }

    Widget buildField(
      String label,
      TextEditingController controller,
      String errorMsg, {
      String? Function(String?)? validator,
      bool autofocus = false,
      String? hintText,
      String? helperText,
    }) {
      if (isDesktop || isWeb) {
        return Row(
          children: [
            SizedBox(
              width: 120,
              child: Text(label),
            ),
            SizedBox(width: 8),
            Expanded(
              child: serverSettingsTextFormField(
                label: label,
                controller: controller,
                errorMsg: errorMsg,
                contentPadding:
                    EdgeInsets.symmetric(horizontal: 8, vertical: 12),
                showLabelText: false,
                validator: validator,
                autofocus: autofocus,
                hintText: hintText,
                helperText: helperText,
              ).workaroundFreezeLinuxMint(),
            ),
          ],
        );
      }

      return serverSettingsTextFormField(
        label: label,
        controller: controller,
        errorMsg: errorMsg,
        validator: validator,
        hintText: hintText,
        helperText: helperText,
      ).workaroundFreezeLinuxMint();
    }

    return CustomAlertDialog(
      title: Row(
        children: [
          Expanded(child: Text(translate('ID/Relay Server'))),
          ...ServerConfigImportExportWidgets(controllers, errMsgs),
        ],
      ),
      content: ConstrainedBox(
        constraints: const BoxConstraints(minWidth: 500),
        child: Form(
          child: Obx(() => Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(
                    'Enter your ID/relay server values. Relay and key can stay empty when your deployment does not use them. Use the official RustDesk API for an existing RustDesk account, or a compatible self-hosted API for login, address books, and synced settings.',
                    style: Theme.of(context).textTheme.bodySmall,
                  ),
                  SizedBox(height: 12),
                  managedServerPanel(),
                  SizedBox(height: 12),
                  buildField(
                    translate('ID Server'),
                    idCtrl,
                    idServerMsg.value,
                    autofocus: true,
                    hintText: 'hbbs.example.com',
                    helperText: 'Host and optional port for hbbs.',
                  ),
                  SizedBox(height: 8),
                  if (!isIOS && !isWeb) ...[
                    buildField(
                      translate('Relay Server'),
                      relayCtrl,
                      relayServerMsg.value,
                      hintText: 'hbbr.example.com',
                      helperText: 'Host and optional port for hbbr.',
                    ),
                    SizedBox(height: 8),
                  ],
                  buildField(
                    translate('API Server'),
                    apiCtrl,
                    apiServerMsg.value,
                    hintText: 'https://admin.rustdesk.com',
                    helperText:
                        'Account API endpoint. Managed hbbs/hbbr uses the official RustDesk API by default unless you enter your own compatible API server.',
                    validator: (v) {
                      if (v != null && v.isNotEmpty) {
                        if (!(v.startsWith('http://') ||
                            v.startsWith("https://"))) {
                          return translate("invalid_http");
                        }
                      }
                      return null;
                    },
                  ),
                  SizedBox(height: 8),
                  buildField(
                    'Key',
                    keyCtrl,
                    '',
                    helperText:
                        'Optional server public key from your deployment.',
                  ),
                  if (isInProgress)
                    Padding(
                      padding: EdgeInsets.only(top: 8),
                      child: LinearProgressIndicator(),
                    ),
                ],
              )),
        ),
      ),
      actions: [
        dialogButton('Cancel', onPressed: () {
          close();
        }, isOutline: true),
        dialogButton(
          'OK',
          onPressed: () async {
            if (await submit()) {
              close();
              showToast(translate('Successful'));
              upSetState?.call(() {});
            } else {
              showToast(translate('Failed'));
            }
          },
        ),
      ],
    );
  });
}

TextFormField serverSettingsTextFormField({
  required String label,
  required TextEditingController controller,
  required String errorMsg,
  String? Function(String?)? validator,
  bool autofocus = false,
  bool showLabelText = true,
  EdgeInsetsGeometry? contentPadding,
  String? hintText,
  String? helperText,
}) {
  return TextFormField(
    controller: controller,
    decoration: InputDecoration(
      labelText: showLabelText ? label : null,
      errorText: errorMsg.isEmpty ? null : errorMsg,
      contentPadding: contentPadding,
      hintText: hintText,
      helperText: helperText,
    ),
    validator: validator,
    autofocus: autofocus,
    keyboardType: TextInputType.visiblePassword,
    textCapitalization: TextCapitalization.none,
    autocorrect: false,
    enableSuggestions: false,
    smartDashesType: SmartDashesType.disabled,
    smartQuotesType: SmartQuotesType.disabled,
    enableIMEPersonalizedLearning: false,
    spellCheckConfiguration: const SpellCheckConfiguration.disabled(),
  );
}

void setPrivacyModeDialog(
  OverlayDialogManager dialogManager,
  List<TToggleMenu> privacyModeList,
  RxString privacyModeState,
) async {
  dialogManager.dismissAll();
  dialogManager.show((setState, close, context) {
    return CustomAlertDialog(
      title: Text(translate('Privacy mode')),
      content: Column(
          mainAxisAlignment: MainAxisAlignment.spaceEvenly,
          children: privacyModeList
              .map((value) => CheckboxListTile(
                    contentPadding: EdgeInsets.zero,
                    visualDensity: VisualDensity.compact,
                    title: value.child,
                    value: value.value,
                    onChanged: value.onChanged,
                  ))
              .toList()),
    );
  }, backDismiss: true, clickMaskDismiss: true);
}
