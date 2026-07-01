import 'dart:async';
import 'dart:convert';

import 'package:bot_toast/bot_toast.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hbb/common/hbbs/hbbs.dart';
import 'package:flutter_hbb/models/ab_model.dart';
import 'package:get/get.dart';

import '../common.dart';
import '../utils/http_service.dart' as http;
import 'model.dart';
import 'platform_model.dart';

bool refreshingUser = false;
bool refreshingAccountServerConfig = false;
bool accountServerConfigRefreshAttempted = false;
bool refreshingOtherModels = false;

class UserModel {
  final RxString userName = ''.obs;
  final RxString displayName = ''.obs;
  final RxString avatar = ''.obs;
  final RxBool isAdmin = false.obs;
  final RxString networkError = ''.obs;
  bool get isLogin => userName.isNotEmpty;
  String get displayNameOrUserName =>
      displayName.value.trim().isEmpty ? userName.value : displayName.value;
  String get accountLabelWithHandle {
    final username = userName.value.trim();
    if (username.isEmpty) {
      return '';
    }
    final preferred = displayName.value.trim();
    if (preferred.isEmpty || preferred == username) {
      return username;
    }
    return '$preferred (@$username)';
  }

  WeakReference<FFI> parent;

  UserModel(this.parent) {
    userName.listen((p0) {
      // When user name becomes empty, show login button
      // When user name becomes non-empty:
      //  For _updateLocalUserInfo, network error will be set later
      //  For login success, should clear network error
      networkError.value = '';
      if (p0.isNotEmpty) {
        Future.delayed(const Duration(milliseconds: 500), () {
          updateOtherModels(quiet: true);
        });
      }
    });
  }

  void refreshCurrentUser() async {
    if (bind.isDisableAccount()) return;
    networkError.value = '';
    final token = bind.mainGetLocalOption(key: 'access_token');
    if (token == '') {
      await updateOtherModels();
      return;
    }
    _updateLocalUserInfo();
    final url = await bind.mainGetApiServer();
    final body = {
      'id': await bind.mainGetMyId(),
      'uuid': await bind.mainGetUuid(),
      'deviceInfo': _loginDeviceInfo(),
    };
    if (refreshingUser) return;
    try {
      refreshingUser = true;
      final http.Response response;
      try {
        response = await http.post(Uri.parse('$url/api/currentUser'),
            headers: {
              'Content-Type': 'application/json',
              'Authorization': 'Bearer $token'
            },
            body: json.encode(body));
      } catch (e) {
        networkError.value = e.toString();
        rethrow;
      }
      refreshingUser = false;
      final status = response.statusCode;
      if (status == 401 || status == 400) {
        reset(resetOther: status == 401);
        return;
      }
      final data = json.decode(decode_http_response(response));
      final error = data['error'];
      if (error != null) {
        throw error;
      }

      final user = UserPayload.fromJson(data);
      _parseAndUpdateUser(user);
      await refreshServerConfigFromAccount();
    } catch (e) {
      debugPrint('Failed to refreshCurrentUser: $e');
    } finally {
      refreshingUser = false;
      await updateOtherModels();
    }
  }

  static Map<String, dynamic>? getLocalUserInfo() {
    final userInfo = bind.mainGetLocalOption(key: 'user_info');
    if (userInfo == '') {
      return null;
    }
    try {
      return json.decode(userInfo);
    } catch (e) {
      debugPrint('Failed to get local user info "$userInfo": $e');
    }
    return null;
  }

  _updateLocalUserInfo() {
    final userInfo = getLocalUserInfo();
    if (userInfo != null) {
      userName.value = (userInfo['name'] ?? '').toString();
      displayName.value = (userInfo['display_name'] ?? '').toString();
      avatar.value = (userInfo['avatar'] ?? '').toString();
    }
  }

  Future<void> reset({bool resetOther = false}) async {
    await bind.mainSetLocalOption(key: 'access_token', value: '');
    await bind.mainSetLocalOption(key: 'user_info', value: '');
    if (resetOther) {
      await gFFI.abModel.reset();
      await gFFI.groupModel.reset();
    }
    userName.value = '';
    displayName.value = '';
    avatar.value = '';
  }

  _parseAndUpdateUser(UserPayload user) {
    userName.value = user.name;
    displayName.value = user.displayName;
    avatar.value = user.avatar;
    isAdmin.value = user.isAdmin;
    bind.mainSetLocalOption(key: 'user_info', value: jsonEncode(user));
    if (isWeb) {
      // ugly here, tmp solution
      bind.mainSetLocalOption(key: 'verifier', value: user.verifier ?? '');
    }
  }

  // update ab and group status
  static Future<void> updateOtherModels({bool quiet = false}) async {
    if (refreshingOtherModels) return;
    refreshingOtherModels = true;
    try {
      await gFFI.userModel
          .refreshServerConfigFromAccount(force: true, refreshModels: false);
      await gFFI.groupModel.pull(force: true, quiet: quiet);
      await gFFI.abModel
          .pullAb(force: ForcePullAb.listAndCurrent, quiet: quiet);
    } finally {
      refreshingOtherModels = false;
    }
  }

  Future<void> logOut({String? apiServer}) async {
    final tag = gFFI.dialogManager.showLoading(translate('Waiting'));
    try {
      final url = apiServer ?? await bind.mainGetApiServer();
      final authHeaders = getHttpHeaders();
      authHeaders['Content-Type'] = "application/json";
      await http
          .post(Uri.parse('$url/api/logout'),
              body: jsonEncode({
                'id': await bind.mainGetMyId(),
                'uuid': await bind.mainGetUuid(),
              }),
              headers: authHeaders)
          .timeout(Duration(seconds: 2));
    } catch (e) {
      debugPrint("request /api/logout failed: err=$e");
    } finally {
      await reset(resetOther: true);
      gFFI.dialogManager.dismissByTag(tag);
    }
  }

  /// throw [RequestException]
  Future<LoginResponse> login(LoginRequest loginRequest) async {
    final url = await bind.mainGetApiServer();
    final resp = await _postLogin(url, loginRequest);

    final Map<String, dynamic> body;
    try {
      body = jsonDecode(decode_http_response(resp));
    } catch (e) {
      debugPrint("login: jsonDecode resp body failed: ${e.toString()}");
      if (resp.statusCode != 200) {
        BotToast.showText(
            contentColor: Colors.red, text: 'HTTP ${resp.statusCode}');
      }
      throw RequestException(resp.statusCode,
          'Account API returned an unreadable response from $url. Check that the Self-hosted API server points to the BeyondRemote account API, not hbbs or hbbr.');
    }
    if (resp.statusCode != 200) {
      throw RequestException(resp.statusCode, body['error'] ?? '');
    }
    if (body['error'] != null) {
      throw RequestException(0, body['error']);
    }

    return getLoginResponseFromAuthBody(body);
  }

  Future<http.Response> _postLogin(
      String url, LoginRequest loginRequest) async {
    try {
      return await http.post(Uri.parse('$url/api/login'),
          body: jsonEncode(loginRequest.toJson()));
    } catch (e) {
      final lower = e.toString().toLowerCase();
      if (lower.contains('timedout') ||
          lower.contains('timed out') ||
          lower.contains('connection refused') ||
          lower.contains('failed host lookup') ||
          lower.contains('the http request failed')) {
        throw RequestException(0,
            'Cannot reach the Beyond Remote account API at $url. Login, address books, and synced settings require the hosted Beyond Remote API or another compatible account API. ID/relay servers still support direct remote connections by ID and password.');
      }
      rethrow;
    }
  }

  LoginResponse getLoginResponseFromAuthBody(Map<String, dynamic> body) {
    final LoginResponse loginResponse;
    try {
      loginResponse = LoginResponse.fromJson(body);
    } catch (e) {
      debugPrint("login: jsonDecode LoginResponse failed: ${e.toString()}");
      rethrow;
    }

    final isLogInDone = loginResponse.type == HttpType.kAuthResTypeToken &&
        loginResponse.access_token != null;
    if (isLogInDone && loginResponse.user != null) {
      _parseAndUpdateUser(loginResponse.user!);
    }

    return loginResponse;
  }

  Future<bool> refreshServerConfigFromAccount(
      {bool force = false, bool refreshModels = true}) async {
    if (bind.isDisableAccount()) return false;
    if (accountServerConfigRefreshAttempted && !force) return false;
    if (refreshingAccountServerConfig) return false;

    final token = bind.mainGetLocalOption(key: 'access_token');
    if (token.isEmpty) return false;

    accountServerConfigRefreshAttempted = true;
    refreshingAccountServerConfig = true;
    try {
      final url = await bind.mainGetApiServer();
      if (url.trim().isEmpty) return false;
      final modifiedAt = force
          ? 0
          : int.tryParse(bind.mainGetLocalOption(key: 'strategy_timestamp')) ??
              0;
      final headers = getHttpHeaders();
      headers['Content-Type'] = 'application/json';
      final response = await http.post(Uri.parse('$url/api/heartbeat'),
          headers: headers,
          body: jsonEncode({
            'id': await bind.mainGetMyId(),
            'uuid': await bind.mainGetUuid(),
            'deviceInfo': _loginDeviceInfo(),
            'modified_at': modifiedAt,
          }));
      if (response.statusCode != 200) {
        debugPrint(
            'refreshServerConfigFromAccount: HTTP ${response.statusCode}');
        return false;
      }

      final data = jsonDecode(decode_http_response(response));
      if (data is! Map<String, dynamic>) return false;
      final rspModifiedAt = data['modified_at'];
      if (rspModifiedAt != null) {
        await bind.mainSetLocalOption(
            key: 'strategy_timestamp', value: rspModifiedAt.toString());
      }

      final strategy = data['strategy'];
      if (strategy is! Map<String, dynamic>) return false;
      final configOptions = strategy['config_options'];
      if (configOptions is! Map<String, dynamic> || configOptions.isEmpty) {
        return false;
      }
      final localManagedConfig = await _localManagedServerConfig();
      for (final entry in configOptions.entries) {
        final key = entry.key.toString();
        final localValue = localManagedConfig[key];
        await bind.mainSetOption(
            key: key, value: localValue ?? entry.value?.toString() ?? '');
      }
      if (force && refreshModels) {
        await updateOtherModels(quiet: true);
      }
      return true;
    } catch (e) {
      debugPrint('refreshServerConfigFromAccount failed: $e');
      return false;
    } finally {
      refreshingAccountServerConfig = false;
    }
  }

  Future<Map<String, String>> _localManagedServerConfig() async {
    try {
      final raw = await bind.mainGetCommon(key: 'managed-server-status');
      final status = jsonDecode(raw);
      if (status is! Map<String, dynamic> ||
          status['enabled'] != true ||
          status['running'] != true ||
          (status['public_host']?.toString().trim().isEmpty ?? true)) {
        return {};
      }
      return {
        'custom-rendezvous-server': '127.0.0.1:21116',
        'relay-server': '127.0.0.1:21117',
      };
    } catch (_) {
      return {};
    }
  }

  Map<String, dynamic> _loginDeviceInfo() {
    try {
      final info = jsonDecode(bind.mainGetLoginDeviceInfo());
      if (info is Map<String, dynamic>) return info;
    } catch (e) {
      debugPrint('Failed to decode login device info: $e');
    }
    return {};
  }

  static Future<List<dynamic>> queryOidcLoginOptions() async {
    try {
      final url = await bind.mainGetApiServer();
      if (url.trim().isEmpty) return [];
      final resp = await http.get(Uri.parse('$url/api/login-options'));
      final List<String> ops = [];
      for (final item in jsonDecode(resp.body)) {
        ops.add(item as String);
      }
      for (final item in ops) {
        if (item.startsWith('common-oidc/')) {
          return jsonDecode(item.substring('common-oidc/'.length));
        }
      }
      return ops
          .where((item) => item.startsWith('oidc/'))
          .map((item) => {'name': item.substring('oidc/'.length)})
          .toList();
    } catch (e) {
      debugPrint(
          "queryOidcLoginOptions: jsonDecode resp body failed: ${e.toString()}");
      return [];
    }
  }
}
