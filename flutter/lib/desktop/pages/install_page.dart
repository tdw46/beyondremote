import 'dart:convert';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hbb/common.dart';
import 'package:flutter_hbb/desktop/widgets/tabbar_widget.dart';
import 'package:flutter_hbb/models/platform_model.dart';
import 'package:flutter_hbb/models/state_model.dart';
import 'package:get/get.dart';
import 'package:path/path.dart';
import 'package:url_launcher/url_launcher_string.dart';
import 'package:window_manager/window_manager.dart';

class InstallPage extends StatefulWidget {
  const InstallPage({Key? key}) : super(key: key);

  @override
  State<InstallPage> createState() => _InstallPageState();
}

class _InstallPageState extends State<InstallPage> {
  final tabController = DesktopTabController(tabType: DesktopTabType.main);

  _InstallPageState() {
    Get.put<DesktopTabController>(tabController);
    const label = "install";
    tabController.add(TabInfo(
        key: label,
        label: label,
        closable: false,
        page: _InstallPageBody(
          key: const ValueKey(label),
        )));
  }

  @override
  void dispose() {
    super.dispose();
    Get.delete<DesktopTabController>();
  }

  @override
  Widget build(BuildContext context) {
    return DragToResizeArea(
      resizeEdgeSize: stateGlobal.resizeEdgeSize.value,
      enableResizeEdges: windowManagerEnableResizeEdges,
      child: Container(
        child: Scaffold(
            backgroundColor: Theme.of(context).colorScheme.background,
            body: DesktopTab(controller: tabController)),
      ),
    );
  }
}

class _InstallPageBody extends StatefulWidget {
  const _InstallPageBody({Key? key}) : super(key: key);

  @override
  State<_InstallPageBody> createState() => _InstallPageBodyState();
}

class _InstallPageBodyState extends State<_InstallPageBody>
    with WindowListener {
  late final TextEditingController controller;
  final RxBool currentUser = false.obs;
  final RxString installError = ''.obs;
  final RxBool startmenu = true.obs;
  final RxBool desktopicon = true.obs;
  final RxBool printer = false.obs;
  final RxBool showProgress = false.obs;
  final RxBool btnEnabled = true.obs;

  // todo move to theme.
  final buttonStyle = OutlinedButton.styleFrom(
    textStyle: TextStyle(fontSize: 14, fontWeight: FontWeight.normal),
    padding: EdgeInsets.symmetric(vertical: 15, horizontal: 12),
  );

  _InstallPageBodyState() {
    controller = TextEditingController(text: bind.installInstallPath());
    final installOptions = jsonDecode(bind.installInstallOptions());
    startmenu.value = installOptions['STARTMENUSHORTCUTS'] != '0';
    desktopicon.value = installOptions['DESKTOPSHORTCUTS'] != '0';
    printer.value = installOptions['PRINTER'] == '1';
  }

  @override
  void initState() {
    windowManager.addListener(this);
    super.initState();
  }

  @override
  void dispose() {
    windowManager.removeListener(this);
    controller.dispose();
    super.dispose();
  }

  @override
  void onWindowClose() {
    gFFI.close();
    super.onWindowClose();
    windowManager.setPreventClose(false);
    windowManager.close();
  }

  InkWell Option(RxBool option, {String label = ''}) {
    return InkWell(
      // todo mouseCursor: "SystemMouseCursors.forbidden" or no cursor on btnEnabled == false
      borderRadius: BorderRadius.circular(6),
      onTap: () => btnEnabled.value ? option.value = !option.value : null,
      child: Row(
        children: [
          Obx(
            () => Checkbox(
              visualDensity: VisualDensity(horizontal: -4, vertical: -4),
              value: option.value,
              onChanged: (v) =>
                  btnEnabled.value ? option.value = !option.value : null,
            ).marginOnly(right: 8),
          ),
          Expanded(
            child: Text(translate(label)),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final double em = 13;
    final isDarkTheme = MyTheme.currentThemeMode() == ThemeMode.dark;
    return Scaffold(
        backgroundColor: null,
        body: SingleChildScrollView(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(translate('Installation'),
                  style: Theme.of(context).textTheme.headlineMedium),
              if (isWindows)
                Obx(() => Column(children: [
                      RadioListTile<bool>(
                        title: const Text('System (recommended)'),
                        subtitle: const Text(
                            'All users. Starts with Windows and supports control of administrator windows. Windows asks for administrator approval.'),
                        value: false,
                        groupValue: currentUser.value,
                        onChanged: btnEnabled.value
                            ? (_) => _changeScope(false)
                            : null,
                      ),
                      RadioListTile<bool>(
                        title: const Text('Just for me'),
                        subtitle: const Text(
                            'No administrator installation. Runs while you are signed in and the app is open. Cannot control elevated windows or the sign-in screen. To uninstall, close the app and remove its folder.'),
                        value: true,
                        groupValue: currentUser.value,
                        onChanged:
                            btnEnabled.value ? (_) => _changeScope(true) : null,
                      ),
                    ])),
              Row(
                children: [
                  Text('${translate('Installation Path')}:')
                      .marginOnly(right: 10),
                  Expanded(
                    child: TextField(
                      controller: controller,
                      readOnly: true,
                      decoration: InputDecoration(
                        contentPadding: EdgeInsets.all(0.75 * em),
                      ),
                    ).workaroundFreezeLinuxMint().marginOnly(right: 10),
                  ),
                  Obx(
                    () => OutlinedButton.icon(
                      icon: Icon(Icons.folder_outlined, size: 16),
                      onPressed: btnEnabled.value ? selectInstallPath : null,
                      style: buttonStyle,
                      label: Text(translate('Change Path')),
                    ),
                  )
                ],
              ).marginSymmetric(vertical: 2 * em),
              Option(startmenu, label: 'Create start menu shortcuts')
                  .marginOnly(bottom: 7),
              Option(desktopicon, label: 'Create desktop icon')
                  .marginOnly(bottom: 7),
              Obx(() => currentUser.value
                  ? const SizedBox.shrink()
                  : Option(printer, label: 'Install {$appName} Printer')),
              Obx(() => installError.value.isEmpty
                  ? const SizedBox.shrink()
                  : Padding(
                      padding: const EdgeInsets.all(12),
                      child: SelectableText(installError.value))),
              Container(
                  padding: EdgeInsets.all(12),
                  decoration: BoxDecoration(
                    color: isDarkTheme
                        ? Color.fromARGB(135, 87, 87, 90)
                        : Colors.grey[100],
                    borderRadius: BorderRadius.circular(8),
                    border: Border.all(color: Colors.grey),
                  ),
                  child: Row(
                    children: [
                      Icon(Icons.info_outline_rounded, size: 32)
                          .marginOnly(right: 16),
                      Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(translate('agreement_tip'))
                              .marginOnly(bottom: em),
                          InkWell(
                            hoverColor: Colors.transparent,
                            onTap: () => launchUrlString(
                                'https://rustdesk.com/privacy.html'),
                            child: Tooltip(
                              message: 'https://rustdesk.com/privacy.html',
                              child: Row(children: [
                                Icon(Icons.launch_outlined, size: 16)
                                    .marginOnly(right: 5),
                                Text(
                                  translate('End-user license agreement'),
                                  style: const TextStyle(
                                      decoration: TextDecoration.underline),
                                )
                              ]),
                            ),
                          ),
                        ],
                      )
                    ],
                  )).marginSymmetric(vertical: 2 * em),
              Row(
                children: [
                  Expanded(
                    // NOT use Offstage to wrap LinearProgressIndicator
                    child: Obx(() => showProgress.value
                        ? LinearProgressIndicator().marginOnly(right: 10)
                        : Offstage()),
                  ),
                  Obx(
                    () => OutlinedButton.icon(
                      icon: Icon(Icons.close_rounded, size: 16),
                      label: Text(translate('Cancel')),
                      onPressed:
                          btnEnabled.value ? () => windowManager.close() : null,
                      style: buttonStyle,
                    ).marginOnly(right: 10),
                  ),
                  Obx(
                    () => ElevatedButton.icon(
                      icon: Icon(Icons.done_rounded, size: 16),
                      label: Text(translate('Accept and Install')),
                      onPressed: btnEnabled.value ? install : null,
                      style: buttonStyle,
                    ),
                  ),
                  Offstage(
                    offstage: bind.installShowRunWithoutInstall(),
                    child: Obx(
                      () => OutlinedButton.icon(
                        icon: Icon(Icons.screen_share_outlined, size: 16),
                        label: Text(translate('Run without install')),
                        onPressed: btnEnabled.value
                            ? () => bind.installRunWithoutInstall()
                            : null,
                        style: buttonStyle,
                      ).marginOnly(left: 10),
                    ),
                  ),
                ],
              )
            ],
          ).paddingSymmetric(horizontal: 4 * em, vertical: 3 * em),
        ));
  }

  Future<void> _changeScope(bool value) async {
    btnEnabled.value = false;
    try {
      final path = value
          ? await bind.mainGetCommon(key: 'user-install-path')
          : bind.installInstallPath();
      if (!mounted) return;
      if (path.isEmpty) {
        installError.value = 'Could not locate your user installation folder.';
        return;
      }
      currentUser.value = value;
      if (value) printer.value = false;
      controller.text = path;
      installError.value = '';
    } finally {
      if (mounted) btnEnabled.value = true;
    }
  }

  Future<void> install() async {
    if (!btnEnabled.value) return;
    btnEnabled.value = false;
    showProgress.value = true;
    installError.value = '';
    String args = currentUser.value ? ' currentuser' : '';
    if (startmenu.value) args += ' startmenu';
    if (desktopicon.value) args += ' desktopicon';
    if (printer.value && !currentUser.value) args += ' printer';
    await bind.installInstallMe(options: args, path: controller.text);
    // Local IPC only, bounded to this explicit installation. Success relaunches
    // the installed app; cancellation/errors keep this form and path intact.
    for (var i = 0; i < 120 && mounted; i++) {
      await Future<void>.delayed(const Duration(seconds: 1));
      if (!mounted) return;
      final result = await bind.mainGetAsyncStatus();
      if (result.trim().isNotEmpty) {
        installError.value = result;
        showProgress.value = false;
        btnEnabled.value = true;
        return;
      }
    }
    if (mounted) {
      installError.value =
          'Still waiting for the installer. Review the Windows approval window before closing or retrying.';
    }
  }

  void selectInstallPath() async {
    String? install_path = await FilePicker.platform
        .getDirectoryPath(initialDirectory: controller.text);
    if (install_path != null) {
      controller.text = join(install_path, await bind.mainGetAppName());
    }
  }
}
