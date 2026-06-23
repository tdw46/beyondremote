<?php
declare(strict_types=1);

if (getenv('BR_SYNC_DEBUG') === '1') {
    ini_set('display_errors', '1');
    error_reporting(E_ALL);
}

const PERSONAL_AB_GUID = 'personal';

main();

function main(): void
{
    try {
        $method = $_SERVER['REQUEST_METHOD'] ?? 'GET';
        $path = parse_url($_SERVER['REQUEST_URI'] ?? '/', PHP_URL_PATH) ?: '/';
        if ($method === 'OPTIONS') {
            send_json(null, 204);
        }

        route($method, $path);
    } catch (HttpError $err) {
        send_json(['error' => $err->getMessage()], $err->statusCode);
    } catch (Throwable $err) {
        error_log('Beyond Remote sync API error: ' . $err->getMessage());
        $message = getenv('BR_SYNC_DEBUG') === '1' ? $err->getMessage() : 'Internal server error';
        send_json(['error' => $message], 500);
    }
}

function route(string $method, string $path): void
{
    if ($method === 'GET' && $path === '/api/beyondremote/ping') {
        $pdo = db();
        send_json([
            'ok' => true,
            'service' => 'Beyond Remote Sync API',
            'db' => $pdo->getAttribute(PDO::ATTR_DRIVER_NAME),
            'providers' => enabled_provider_names(),
        ]);
    }
    if ($method === 'POST' && $path === '/api/beyondremote/device-auth') {
        device_auth();
    }

    if ($method === 'GET' && $path === '/api/login-options') {
        send_json(array_map(function ($op) {
            return 'oidc/' . $op;
        }, enabled_provider_names()));
    }

    if ($method === 'POST' && $path === '/api/login') {
        password_login();
    }
    if ($method === 'POST' && $path === '/api/currentUser') {
        current_user();
    }
    if ($method === 'POST' && $path === '/api/logout') {
        logout();
    }
    if ($method === 'POST' && $path === '/api/oidc/auth') {
        oidc_auth();
    }
    if ($method === 'GET' && $path === '/api/oidc/auth-query') {
        oidc_auth_query();
    }
    if ($method === 'GET' && preg_match('#^/auth/(github|google)/start$#', $path, $m)) {
        oauth_start($m[1]);
    }
    if ($method === 'GET' && preg_match('#^/auth/(github|google)/callback$#', $path, $m)) {
        oauth_callback($m[1]);
    }

    if (($method === 'GET' && $path === '/api/ab') || ($method === 'POST' && $path === '/api/ab/get')) {
        legacy_ab_get();
    }
    if ($method === 'POST' && $path === '/api/ab') {
        legacy_ab_set();
    }
    if ($method === 'POST' && $path === '/api/ab/settings') {
        require_user();
        send_json(['max_peer_one_ab' => 0]);
    }
    if ($method === 'POST' && $path === '/api/ab/personal') {
        require_user();
        send_json(['guid' => PERSONAL_AB_GUID]);
    }
    if ($method === 'POST' && $path === '/api/ab/shared/profiles') {
        require_user();
        send_json(paged([]));
    }
    if ($method === 'POST' && $path === '/api/ab/peers') {
        ab_peers();
    }
    if ($method === 'POST' && preg_match('#^/api/ab/tags/[^/]+$#', $path)) {
        ab_tags();
    }
    if ($method === 'POST' && preg_match('#^/api/ab/peer/add/[^/]+$#', $path)) {
        ab_peer_add();
    }
    if ($method === 'PUT' && preg_match('#^/api/ab/peer/update/[^/]+$#', $path)) {
        ab_peer_update();
    }
    if ($method === 'DELETE' && preg_match('#^/api/ab/peer/[^/]+$#', $path)) {
        ab_peer_delete();
    }
    if ($method === 'POST' && preg_match('#^/api/ab/tag/add/[^/]+$#', $path)) {
        ab_tag_add();
    }
    if ($method === 'PUT' && preg_match('#^/api/ab/tag/rename/[^/]+$#', $path)) {
        ab_tag_rename();
    }
    if ($method === 'PUT' && preg_match('#^/api/ab/tag/update/[^/]+$#', $path)) {
        ab_tag_update();
    }
    if ($method === 'DELETE' && preg_match('#^/api/ab/tag/[^/]+$#', $path)) {
        ab_tag_delete();
    }

    if ($method === 'GET' && $path === '/api/device-group/accessible') {
        require_user();
        send_json(paged([]));
    }
    if ($method === 'GET' && $path === '/api/users') {
        $user = require_user();
        send_json(paged([user_payload($user)]));
    }
    if ($method === 'GET' && $path === '/api/peers') {
        group_peers();
    }

    throw new HttpError('Not found', 404);
}

function config(): array
{
    static $config = null;
    if ($config !== null) {
        return $config;
    }

    $path = getenv('BR_SYNC_CONFIG') ?: dirname(__DIR__) . '/beyondremote-sync-config.php';
    if (!is_file($path)) {
        throw new RuntimeException('Missing Beyond Remote sync config');
    }
    $config = require $path;
    return $config;
}

function db(): PDO
{
    static $pdo = null;
    if ($pdo instanceof PDO) {
        return $pdo;
    }

    $db = config()['db'];
    $pdo = new PDO($db['dsn'], $db['user'], $db['password'], [
        PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION,
        PDO::ATTR_DEFAULT_FETCH_MODE => PDO::FETCH_ASSOC,
    ]);
    migrate($pdo);
    return $pdo;
}

function migrate(PDO $pdo): void
{
    static $done = false;
    if ($done) {
        return;
    }

    $pdo->exec("
        CREATE TABLE IF NOT EXISTS br_users (
            id BIGSERIAL PRIMARY KEY,
            provider TEXT NOT NULL DEFAULT 'password',
            provider_user_id TEXT NOT NULL,
            name TEXT NOT NULL UNIQUE,
            display_name TEXT NOT NULL DEFAULT '',
            email TEXT NOT NULL DEFAULT '',
            avatar TEXT NOT NULL DEFAULT '',
            password_hash TEXT,
            status INTEGER NOT NULL DEFAULT 1,
            is_admin BOOLEAN NOT NULL DEFAULT false,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE(provider, provider_user_id)
        );
        CREATE TABLE IF NOT EXISTS br_sessions (
            token_hash TEXT PRIMARY KEY,
            user_id BIGINT NOT NULL REFERENCES br_users(id) ON DELETE CASCADE,
            expires_at TIMESTAMPTZ NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        CREATE TABLE IF NOT EXISTS br_auth_requests (
            code TEXT PRIMARY KEY,
            op TEXT NOT NULL,
            remote_id TEXT NOT NULL DEFAULT '',
            uuid TEXT NOT NULL DEFAULT '',
            device_info JSONB NOT NULL DEFAULT '{}'::jsonb,
            auth_body JSONB,
            expires_at TIMESTAMPTZ NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        CREATE TABLE IF NOT EXISTS br_legacy_address_books (
            user_id BIGINT PRIMARY KEY REFERENCES br_users(id) ON DELETE CASCADE,
            data TEXT NOT NULL,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        CREATE TABLE IF NOT EXISTS br_personal_peers (
            user_id BIGINT NOT NULL REFERENCES br_users(id) ON DELETE CASCADE,
            peer_id TEXT NOT NULL,
            data JSONB NOT NULL,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            PRIMARY KEY(user_id, peer_id)
        );
        CREATE TABLE IF NOT EXISTS br_personal_tags (
            user_id BIGINT NOT NULL REFERENCES br_users(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            color INTEGER NOT NULL DEFAULT 0,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            PRIMARY KEY(user_id, name)
        );
        CREATE TABLE IF NOT EXISTS br_devices (
            user_id BIGINT NOT NULL REFERENCES br_users(id) ON DELETE CASCADE,
            remote_id TEXT NOT NULL,
            uuid TEXT NOT NULL DEFAULT '',
            device_info JSONB NOT NULL DEFAULT '{}'::jsonb,
            last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            PRIMARY KEY(user_id, remote_id)
        );
    ");
    $done = true;
}

function enabled_provider_names(): array
{
    $oauth = config()['oauth'] ?? [];
    $names = [];
    foreach (['github', 'google'] as $name) {
        if (!empty($oauth[$name]['client_id']) && !empty($oauth[$name]['client_secret'])) {
            $names[] = $name;
        }
    }
    return $names;
}

function provider_config(string $provider): array
{
    $cfg = config()['oauth'][$provider] ?? null;
    if (!$cfg || empty($cfg['client_id']) || empty($cfg['client_secret'])) {
        throw new HttpError('OAuth provider is not configured', 400);
    }
    return $cfg;
}

function password_login(): void
{
    $body = json_body();
    $username = trim((string)($body['username'] ?? ''));
    $password = (string)($body['password'] ?? '');
    if ($username === '') {
        throw new HttpError('Username is required', 400);
    }

    $pdo = db();
    $user = find_user_by_provider('password', $username);
    if (!$user) {
        $hash = password_hash($password, PASSWORD_DEFAULT);
        $user = upsert_user('password', $username, $username, $username, '', '', $hash);
    } elseif (empty($user['password_hash']) || !password_verify($password, $user['password_hash'])) {
        throw new HttpError('Invalid username or password', 401);
    }

    register_device_from_body((int)$user['id'], $body);
    send_json(auth_body($user, create_session((int)$user['id'])));
}

function current_user(): void
{
    $user = require_user();
    register_device_from_body((int)$user['id'], json_body());
    send_json(user_payload($user));
}

function logout(): void
{
    $token = bearer_token();
    if ($token !== '') {
        $stmt = db()->prepare('DELETE FROM br_sessions WHERE token_hash = ?');
        $stmt->execute([hash_token($token)]);
    }
    send_empty_ok();
}

function oidc_auth(): void
{
    $body = json_body();
    $op = strtolower(trim((string)($body['op'] ?? '')));
    if (!in_array($op, enabled_provider_names(), true)) {
        throw new HttpError('OAuth provider is not configured', 400);
    }

    $code = random_token(24);
    $stmt = db()->prepare('
        INSERT INTO br_auth_requests (code, op, remote_id, uuid, device_info, expires_at)
        VALUES (?, ?, ?, ?, ?::jsonb, NOW() + INTERVAL \'10 minutes\')
    ');
    $stmt->execute([
        $code,
        $op,
        (string)($body['id'] ?? ''),
        (string)($body['uuid'] ?? ''),
        json_encode($body['deviceInfo'] ?? new stdClass()),
    ]);

    send_json([
        'code' => $code,
        'url' => api_base_url() . '/auth/' . rawurlencode($op) . '/start?code=' . rawurlencode($code),
    ]);
}

function oidc_auth_query(): void
{
    $code = (string)($_GET['code'] ?? '');
    $stmt = db()->prepare('SELECT auth_body FROM br_auth_requests WHERE code = ? AND expires_at > NOW()');
    $stmt->execute([$code]);
    $row = $stmt->fetch();
    if (!$row || empty($row['auth_body'])) {
        send_json(['error' => 'No authed oidc is found'], 404);
    }
    send_json(normalize_auth_body(json_decode($row['auth_body'], true) ?: []));
}

function oauth_start(string $provider): void
{
    provider_config($provider);
    $code = (string)($_GET['code'] ?? '');
    $request = find_auth_request($code);
    if (!$request || $request['op'] !== $provider) {
        throw new HttpError('OAuth request not found', 404);
    }
    $state = sign_state($provider, $code);

    if ($provider === 'github') {
        $cfg = provider_config('github');
        redirect('https://github.com/login/oauth/authorize?' . http_build_query([
            'client_id' => $cfg['client_id'],
            'redirect_uri' => api_base_url() . '/auth/github/callback',
            'scope' => 'read:user user:email',
            'state' => $state,
        ]));
    }

    $cfg = provider_config('google');
    redirect('https://accounts.google.com/o/oauth2/v2/auth?' . http_build_query([
        'client_id' => $cfg['client_id'],
        'redirect_uri' => api_base_url() . '/auth/google/callback',
        'response_type' => 'code',
        'scope' => 'openid email profile',
        'state' => $state,
        'access_type' => 'online',
        'prompt' => 'select_account',
    ]));
}

function oauth_callback(string $provider): void
{
    $code = (string)($_GET['code'] ?? '');
    $state = (string)($_GET['state'] ?? '');
    [$stateProvider, $requestCode] = verify_state($state);
    if ($stateProvider !== $provider) {
        throw new HttpError('Invalid OAuth state', 400);
    }
    $request = find_auth_request($requestCode);
    if (!$request || $request['op'] !== $provider) {
        throw new HttpError('OAuth request not found', 404);
    }

    $profile = $provider === 'github'
        ? github_profile($code)
        : google_profile($code);

    $user = upsert_user(
        $provider,
        $profile['id'],
        unique_user_name($profile['name'], $provider, $profile['id']),
        $profile['display_name'],
        $profile['email'],
        $profile['avatar'],
        null
    );
    register_device((int)$user['id'], $request['remote_id'], $request['uuid'], json_decode($request['device_info'], true) ?: []);
    $authBody = auth_body($user, create_session((int)$user['id']));

    $stmt = db()->prepare('UPDATE br_auth_requests SET auth_body = ?::jsonb WHERE code = ?');
    $stmt->execute([json_encode($authBody), $requestCode]);

    header('Content-Type: text/html; charset=utf-8');
    echo '<!doctype html><html><head><title>Beyond Remote Login</title></head><body style="font-family:sans-serif;max-width:680px;margin:64px auto;line-height:1.5"><h1>Beyond Remote login complete</h1><p>You can close this tab and return to Beyond Remote.</p></body></html>';
    exit;
}

function github_profile(string $code): array
{
    $cfg = provider_config('github');
    $token = http_json('https://github.com/login/oauth/access_token', [
        'client_id' => $cfg['client_id'],
        'client_secret' => $cfg['client_secret'],
        'code' => $code,
        'redirect_uri' => api_base_url() . '/auth/github/callback',
    ], ['Accept: application/json']);
    if (empty($token['access_token'])) {
        throw new HttpError('GitHub login failed', 400);
    }
    $headers = [
        'Accept: application/vnd.github+json',
        'Authorization: Bearer ' . $token['access_token'],
        'User-Agent: BeyondRemote',
    ];
    $user = http_json('https://api.github.com/user', null, $headers);
    $emails = http_json('https://api.github.com/user/emails', null, $headers);
    $email = '';
    if (is_array($emails)) {
        foreach ($emails as $candidate) {
            if (!empty($candidate['primary']) && !empty($candidate['email'])) {
                $email = (string)$candidate['email'];
                break;
            }
        }
    }
    return [
        'id' => (string)$user['id'],
        'name' => (string)($user['login'] ?? ('github-' . $user['id'])),
        'display_name' => (string)($user['name'] ?: ($user['login'] ?? 'GitHub User')),
        'email' => $email,
        'avatar' => (string)($user['avatar_url'] ?? ''),
    ];
}

function google_profile(string $code): array
{
    $cfg = provider_config('google');
    $token = http_json('https://oauth2.googleapis.com/token', [
        'client_id' => $cfg['client_id'],
        'client_secret' => $cfg['client_secret'],
        'code' => $code,
        'grant_type' => 'authorization_code',
        'redirect_uri' => api_base_url() . '/auth/google/callback',
    ], ['Accept: application/json']);
    if (empty($token['access_token'])) {
        throw new HttpError('Google login failed', 400);
    }
    $user = http_json('https://openidconnect.googleapis.com/v1/userinfo', null, [
        'Authorization: Bearer ' . $token['access_token'],
    ]);
    $email = (string)($user['email'] ?? '');
    return [
        'id' => (string)$user['sub'],
        'name' => $email !== '' ? explode('@', $email)[0] : 'google-' . $user['sub'],
        'display_name' => (string)($user['name'] ?? $email),
        'email' => $email,
        'avatar' => (string)($user['picture'] ?? ''),
    ];
}

function legacy_ab_get(): void
{
    $user = require_user();
    $stmt = db()->prepare('SELECT data, EXTRACT(EPOCH FROM updated_at) * 1000 AS updated_at FROM br_legacy_address_books WHERE user_id = ?');
    $stmt->execute([(int)$user['id']]);
    $row = $stmt->fetch();
    send_json([
        'data' => $row['data'] ?? '{"tags":[],"peers":[],"tag_colors":"{}"}',
        'updated_at' => isset($row['updated_at']) ? (int)$row['updated_at'] : now_millis(),
    ]);
}

function legacy_ab_set(): void
{
    $user = require_user();
    $body = json_body();
    $data = (string)($body['data'] ?? '{"tags":[],"peers":[],"tag_colors":"{}"}');
    $stmt = db()->prepare('
        INSERT INTO br_legacy_address_books (user_id, data, updated_at)
        VALUES (?, ?, NOW())
        ON CONFLICT (user_id) DO UPDATE SET data = EXCLUDED.data, updated_at = NOW()
    ');
    $stmt->execute([(int)$user['id'], $data]);
    sync_legacy_ab_to_personal((int)$user['id'], $data);
    send_empty_ok();
}

function ab_peers(): void
{
    $user = require_user();
    $stmt = db()->prepare('SELECT data FROM br_personal_peers WHERE user_id = ? ORDER BY peer_id');
    $stmt->execute([(int)$user['id']]);
    $rows = $stmt->fetchAll();
    send_json(paged(array_map(function ($row) {
        return json_decode($row['data'], true);
    }, $rows)));
}

function ab_tags(): void
{
    $user = require_user();
    $stmt = db()->prepare('SELECT name, color FROM br_personal_tags WHERE user_id = ? ORDER BY name');
    $stmt->execute([(int)$user['id']]);
    send_json($stmt->fetchAll());
}

function ab_peer_add(): void
{
    $user = require_user();
    upsert_peer((int)$user['id'], json_body(), false);
    send_empty_ok();
}

function ab_peer_update(): void
{
    $user = require_user();
    upsert_peer((int)$user['id'], json_body(), true);
    send_empty_ok();
}

function ab_peer_delete(): void
{
    $user = require_user();
    $ids = json_body();
    if (!is_array($ids)) {
        $ids = [];
    }
    $stmt = db()->prepare('DELETE FROM br_personal_peers WHERE user_id = ? AND peer_id = ?');
    foreach ($ids as $id) {
        $stmt->execute([(int)$user['id'], (string)$id]);
    }
    send_empty_ok();
}

function ab_tag_add(): void
{
    $user = require_user();
    $body = json_body();
    $name = trim((string)($body['name'] ?? ''));
    if ($name === '') {
        throw new HttpError('Missing tag name', 400);
    }
    $color = (int)($body['color'] ?? tag_color($name));
    $stmt = db()->prepare('
        INSERT INTO br_personal_tags (user_id, name, color, updated_at)
        VALUES (?, ?, ?, NOW())
        ON CONFLICT (user_id, name) DO UPDATE SET color = EXCLUDED.color, updated_at = NOW()
    ');
    $stmt->execute([(int)$user['id'], $name, $color]);
    send_empty_ok();
}

function ab_tag_rename(): void
{
    $user = require_user();
    $body = json_body();
    $old = (string)($body['old'] ?? '');
    $new = (string)($body['new'] ?? '');
    if ($old === '' || $new === '') {
        throw new HttpError('Missing tag name', 400);
    }
    $pdo = db();
    $pdo->beginTransaction();
    $stmt = $pdo->prepare('UPDATE br_personal_tags SET name = ?, updated_at = NOW() WHERE user_id = ? AND name = ?');
    $stmt->execute([$new, (int)$user['id'], $old]);
    foreach (load_peer_rows((int)$user['id']) as $row) {
        $peer = json_decode($row['data'], true) ?: [];
        if (!empty($peer['tags']) && is_array($peer['tags'])) {
            $peer['tags'] = array_map(function ($tag) use ($old, $new) {
                return $tag === $old ? $new : $tag;
            }, $peer['tags']);
            save_peer((int)$user['id'], $row['peer_id'], $peer);
        }
    }
    $pdo->commit();
    send_empty_ok();
}

function ab_tag_update(): void
{
    $user = require_user();
    $body = json_body();
    $stmt = db()->prepare('UPDATE br_personal_tags SET color = ?, updated_at = NOW() WHERE user_id = ? AND name = ?');
    $stmt->execute([(int)($body['color'] ?? 0), (int)$user['id'], (string)($body['name'] ?? '')]);
    send_empty_ok();
}

function ab_tag_delete(): void
{
    $user = require_user();
    $names = json_body();
    if (!is_array($names)) {
        $names = [];
    }
    $stmt = db()->prepare('DELETE FROM br_personal_tags WHERE user_id = ? AND name = ?');
    foreach ($names as $name) {
        $stmt->execute([(int)$user['id'], (string)$name]);
    }
    foreach (load_peer_rows((int)$user['id']) as $row) {
        $peer = json_decode($row['data'], true) ?: [];
        if (!empty($peer['tags']) && is_array($peer['tags'])) {
            $peer['tags'] = array_values(array_filter($peer['tags'], function ($tag) use ($names) {
                return !in_array($tag, $names, true);
            }));
            save_peer((int)$user['id'], $row['peer_id'], $peer);
        }
    }
    send_empty_ok();
}

function group_peers(): void
{
    $user = require_user();
    $stmt = db()->prepare('SELECT remote_id, device_info FROM br_devices WHERE user_id = ? ORDER BY last_seen_at DESC');
    $stmt->execute([(int)$user['id']]);
    $peers = [];
    foreach ($stmt->fetchAll() as $row) {
        $info = json_decode($row['device_info'], true) ?: [];
        $peer = load_peer((int)$user['id'], (string)$row['remote_id']) ?: [];
        $alias = trim((string)($peer['alias'] ?? ''));
        $deviceName = $alias !== '' ? $alias : (string)($info['name'] ?? '');
        $peers[] = [
            'id' => $row['remote_id'],
            'status' => 1,
            'user' => $user['name'],
            'user_name' => $user['display_name'] ?: $user['name'],
            'note' => '',
            'info' => [
                'username' => '',
                'os' => (string)($info['os'] ?? ''),
                'device_name' => $deviceName,
            ],
        ];
    }
    send_json(paged($peers));
}

function require_user(): array
{
    $token = bearer_token();
    if ($token === '') {
        throw new HttpError('Unauthorized', 401);
    }
    return require_user_by_token($token);
}

function require_user_by_token(string $token): array
{
    $token = trim($token);
    if ($token === '') {
        throw new HttpError('Unauthorized', 401);
    }
    $stmt = db()->prepare('
        SELECT u.*
        FROM br_sessions s
        JOIN br_users u ON u.id = s.user_id
        WHERE s.token_hash = ? AND s.expires_at > NOW()
    ');
    $stmt->execute([hash_token($token)]);
    $user = $stmt->fetch();
    if (!$user) {
        throw new HttpError('Unauthorized', 401);
    }
    $touch = db()->prepare('UPDATE br_sessions SET last_seen_at = NOW() WHERE token_hash = ?');
    $touch->execute([hash_token($token)]);
    return $user;
}

function device_auth(): void
{
    $caller = require_user();
    $body = json_body();
    $targetToken = (string)($body['target_access_token'] ?? '');
    $sourceId = normalize_remote_id((string)($body['source_id'] ?? ''));
    $targetId = normalize_remote_id((string)($body['target_id'] ?? ''));
    if ($targetToken === '' || $sourceId === '' || $targetId === '') {
        throw new HttpError('Missing device auth fields', 400);
    }
    $target = require_user_by_token($targetToken);
    if ((int)$caller['id'] !== (int)$target['id']) {
        throw new HttpError('Forbidden', 403);
    }
    if (!registered_device_exists((int)$caller['id'], $sourceId)) {
        throw new HttpError('Source device is not registered to this account', 403);
    }
    if (!registered_device_exists((int)$caller['id'], $targetId)) {
        throw new HttpError('Target device is not registered to this account', 403);
    }
    send_json([
        'ok' => true,
        'source_id' => $sourceId,
        'target_id' => $targetId,
    ]);
}

function bearer_token(): string
{
    $header = $_SERVER['HTTP_AUTHORIZATION'] ?? $_SERVER['REDIRECT_HTTP_AUTHORIZATION'] ?? '';
    if (stripos($header, 'Bearer ') !== 0) {
        return '';
    }
    return trim(substr($header, 7));
}

function create_session(int $userId): string
{
    $token = random_token(32);
    $stmt = db()->prepare('
        INSERT INTO br_sessions (token_hash, user_id, expires_at)
        VALUES (?, ?, NOW() + INTERVAL \'90 days\')
    ');
    $stmt->execute([hash_token($token), $userId]);
    return $token;
}

function auth_body(array $user, string $token): array
{
    return normalize_auth_body([
        'type' => 'access_token',
        'access_token' => $token,
        'user' => user_payload($user),
    ]);
}

function normalize_auth_body(array $body): array
{
    if (!isset($body['user']) || !is_array($body['user'])) {
        $body['user'] = [];
    }
    if (!isset($body['user']['info']) || !is_array($body['user']['info'])) {
        $body['user']['info'] = default_user_info();
    }
    if (!isset($body['user']['info']['other']) || empty($body['user']['info']['other'])) {
        $body['user']['info']['other'] = new stdClass();
    }
    return $body;
}

function default_user_info(): array
{
    return [
        'email_verification' => false,
        'email_alarm_notification' => false,
        'login_device_whitelist' => [],
        'other' => new stdClass(),
    ];
}

function user_payload(array $user): array
{
    return [
        'name' => $user['name'],
        'display_name' => $user['display_name'] ?: $user['name'],
        'avatar' => $user['avatar'] ?? '',
        'email' => $user['email'] ?? '',
        'note' => '',
        'status' => (int)$user['status'],
        'is_admin' => (bool)$user['is_admin'],
        'verifier' => '',
        'info' => default_user_info(),
    ];
}

function find_user_by_provider(string $provider, string $providerUserId): ?array
{
    $stmt = db()->prepare('SELECT * FROM br_users WHERE provider = ? AND provider_user_id = ?');
    $stmt->execute([$provider, $providerUserId]);
    $user = $stmt->fetch();
    return $user ?: null;
}

function upsert_user(string $provider, string $providerUserId, string $name, string $displayName, string $email, string $avatar, ?string $passwordHash): array
{
    $existing = find_user_by_provider($provider, $providerUserId);
    if ($existing) {
        $stmt = db()->prepare('
            UPDATE br_users
            SET display_name = ?, email = ?, avatar = ?, password_hash = COALESCE(?, password_hash), updated_at = NOW()
            WHERE id = ?
        ');
        $stmt->execute([$displayName, $email, $avatar, $passwordHash, (int)$existing['id']]);
        $stmt = db()->prepare('SELECT * FROM br_users WHERE id = ?');
        $stmt->execute([(int)$existing['id']]);
        return $stmt->fetch();
    }
    $stmt = db()->prepare('
        INSERT INTO br_users (provider, provider_user_id, name, display_name, email, avatar, password_hash, is_admin)
        VALUES (?, ?, ?, ?, ?, ?, ?, false)
        RETURNING *
    ');
    $stmt->execute([$provider, $providerUserId, $name, $displayName, $email, $avatar, $passwordHash]);
    return $stmt->fetch();
}

function unique_user_name(string $candidate, string $provider, string $providerId): string
{
    $base = strtolower(preg_replace('/[^a-zA-Z0-9_.-]+/', '-', trim($candidate)) ?: $provider . '-' . $providerId);
    $name = trim($base, '-_.');
    if ($name === '') {
        $name = $provider . '-' . $providerId;
    }
    $stmt = db()->prepare('SELECT id FROM br_users WHERE name = ? AND NOT (provider = ? AND provider_user_id = ?)');
    $test = $name;
    $i = 2;
    while (true) {
        $stmt->execute([$test, $provider, $providerId]);
        if (!$stmt->fetch()) {
            return $test;
        }
        $test = $name . '-' . $i;
        $i++;
    }
}

function register_device_from_body(int $userId, array $body): void
{
    register_device($userId, (string)($body['id'] ?? ''), (string)($body['uuid'] ?? ''), $body['deviceInfo'] ?? []);
}

function register_device(int $userId, string $remoteId, string $uuid, array $deviceInfo): void
{
    $remoteId = normalize_remote_id($remoteId);
    if ($remoteId === '') {
        return;
    }
    $existingPeer = load_peer($userId, $remoteId);
    $existingAlias = trim((string)($existingPeer['alias'] ?? ''));
    $deviceName = (string)($deviceInfo['name'] ?? '');
    $stmt = db()->prepare('
        INSERT INTO br_devices (user_id, remote_id, uuid, device_info, last_seen_at)
        VALUES (?, ?, ?, ?::jsonb, NOW())
        ON CONFLICT (user_id, remote_id) DO UPDATE
            SET uuid = EXCLUDED.uuid, device_info = EXCLUDED.device_info, last_seen_at = NOW()
    ');
    $stmt->execute([$userId, $remoteId, $uuid, json_encode($deviceInfo ?: new stdClass())]);

    $peer = [
        'id' => $remoteId,
        'username' => '',
        'hostname' => $deviceName,
        'platform' => map_platform((string)($deviceInfo['os'] ?? '')),
        'alias' => $existingAlias !== '' ? $existingAlias : $deviceName,
        'tags' => ['My devices'],
        'note' => 'Signed in with Beyond Remote',
        'same_server' => true,
    ];
    upsert_peer($userId, $peer, true);
    ensure_tag($userId, 'My devices', tag_color('My devices'));
}

function registered_device_exists(int $userId, string $remoteId): bool
{
    $stmt = db()->prepare('SELECT 1 FROM br_devices WHERE user_id = ? AND remote_id = ? LIMIT 1');
    $stmt->execute([$userId, normalize_remote_id($remoteId)]);
    return (bool)$stmt->fetchColumn();
}

function normalize_remote_id(string $remoteId): string
{
    $remoteId = trim($remoteId);
    $at = strpos($remoteId, '@');
    if ($at !== false) {
        $remoteId = substr($remoteId, 0, $at);
    }
    return trim($remoteId);
}

function upsert_peer(int $userId, array $peer, bool $merge): void
{
    $id = trim((string)($peer['id'] ?? ''));
    if ($id === '') {
        throw new HttpError('Missing peer id', 400);
    }
    if ($merge) {
        $existing = load_peer($userId, $id);
        if ($existing) {
            $peer = array_replace($existing, $peer);
        }
    }
    save_peer($userId, $id, $peer);
}

function load_peer(int $userId, string $peerId): ?array
{
    $stmt = db()->prepare('SELECT data FROM br_personal_peers WHERE user_id = ? AND peer_id = ?');
    $stmt->execute([$userId, $peerId]);
    $row = $stmt->fetch();
    return $row ? (json_decode($row['data'], true) ?: []) : null;
}

function load_peer_rows(int $userId): array
{
    $stmt = db()->prepare('SELECT peer_id, data FROM br_personal_peers WHERE user_id = ?');
    $stmt->execute([$userId]);
    return $stmt->fetchAll();
}

function save_peer(int $userId, string $peerId, array $peer): void
{
    $stmt = db()->prepare('
        INSERT INTO br_personal_peers (user_id, peer_id, data, updated_at)
        VALUES (?, ?, ?::jsonb, NOW())
        ON CONFLICT (user_id, peer_id) DO UPDATE SET data = EXCLUDED.data, updated_at = NOW()
    ');
    $stmt->execute([$userId, $peerId, json_encode($peer)]);
}

function ensure_tag(int $userId, string $name, int $color): void
{
    $stmt = db()->prepare('
        INSERT INTO br_personal_tags (user_id, name, color)
        VALUES (?, ?, ?)
        ON CONFLICT (user_id, name) DO NOTHING
    ');
    $stmt->execute([$userId, $name, $color]);
}

function sync_legacy_ab_to_personal(int $userId, string $data): void
{
    $decoded = json_decode($data, true);
    if (!is_array($decoded)) {
        return;
    }
    foreach (($decoded['peers'] ?? []) as $peer) {
        if (is_array($peer) && !empty($peer['id'])) {
            upsert_peer($userId, $peer, true);
        }
    }
    $tagColors = [];
    if (!empty($decoded['tag_colors'])) {
        $tagColors = is_string($decoded['tag_colors'])
            ? (json_decode($decoded['tag_colors'], true) ?: [])
            : (array)$decoded['tag_colors'];
    }
    foreach (($decoded['tags'] ?? []) as $tag) {
        $name = (string)$tag;
        ensure_tag($userId, $name, (int)($tagColors[$name] ?? tag_color($name)));
    }
}

function find_auth_request(string $code): ?array
{
    $stmt = db()->prepare('SELECT * FROM br_auth_requests WHERE code = ? AND expires_at > NOW()');
    $stmt->execute([$code]);
    $row = $stmt->fetch();
    return $row ?: null;
}

function sign_state(string $provider, string $code): string
{
    $payload = $provider . ':' . $code;
    $sig = hash_hmac('sha256', $payload, config()['app_secret']);
    return base64url_encode($payload . ':' . $sig);
}

function verify_state(string $state): array
{
    $raw = base64url_decode($state);
    $parts = explode(':', $raw);
    if (count($parts) !== 3) {
        throw new HttpError('Invalid OAuth state', 400);
    }
    [$provider, $code, $sig] = $parts;
    $expected = hash_hmac('sha256', $provider . ':' . $code, config()['app_secret']);
    if (!hash_equals($expected, $sig)) {
        throw new HttpError('Invalid OAuth state', 400);
    }
    return [$provider, $code];
}

function http_json(string $url, ?array $postFields, array $headers = []): array
{
    if (function_exists('curl_init')) {
        $ch = curl_init($url);
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_TIMEOUT, 20);
        curl_setopt($ch, CURLOPT_HTTPHEADER, $headers);
        if ($postFields !== null) {
            curl_setopt($ch, CURLOPT_POST, true);
            curl_setopt($ch, CURLOPT_POSTFIELDS, http_build_query($postFields));
        }
        $body = curl_exec($ch);
        $status = (int)curl_getinfo($ch, CURLINFO_HTTP_CODE);
        $err = curl_error($ch);
        curl_close($ch);
        if ($body === false || $status >= 400) {
            throw new HttpError('OAuth HTTP request failed' . ($err ? ': ' . $err : ''), 400);
        }
        return json_decode((string)$body, true) ?: [];
    }

    $opts = ['http' => ['timeout' => 20, 'header' => implode("\r\n", $headers)]];
    if ($postFields !== null) {
        $opts['http']['method'] = 'POST';
        $opts['http']['header'] .= "\r\nContent-Type: application/x-www-form-urlencoded";
        $opts['http']['content'] = http_build_query($postFields);
    }
    $body = file_get_contents($url, false, stream_context_create($opts));
    if ($body === false) {
        throw new HttpError('OAuth HTTP request failed', 400);
    }
    return json_decode($body, true) ?: [];
}

function json_body(): array
{
    $raw = file_get_contents('php://input') ?: '{}';
    $decoded = json_decode($raw, true);
    return is_array($decoded) ? $decoded : [];
}

function send_json($data, int $status = 200): void
{
    http_response_code($status);
    header('Content-Type: application/json; charset=utf-8');
    header('Cache-Control: no-store');
    if ($status !== 204) {
        echo json_encode($data, JSON_UNESCAPED_SLASHES);
    }
    exit;
}

function send_empty_ok(): void
{
    http_response_code(200);
    header('Content-Type: text/plain; charset=utf-8');
    exit;
}

function redirect(string $url): void
{
    header('Location: ' . $url, true, 302);
    exit;
}

function api_base_url(): string
{
    return rtrim((string)config()['api_base_url'], '/');
}

function paged(array $data): array
{
    return ['total' => count($data), 'data' => $data];
}

function random_token(int $bytes): string
{
    return bin2hex(random_bytes($bytes));
}

function hash_token(string $token): string
{
    return hash('sha256', $token);
}

function base64url_encode(string $data): string
{
    return rtrim(strtr(base64_encode($data), '+/', '-_'), '=');
}

function base64url_decode(string $data): string
{
    return base64_decode(strtr($data, '-_', '+/')) ?: '';
}

function now_millis(): int
{
    return (int)floor(microtime(true) * 1000);
}

function tag_color(string $name): int
{
    $hash = 0;
    foreach (str_split($name) as $ch) {
        $hash = (($hash * 31) + ord($ch)) & 0x00ffffff;
    }
    $color = 0xff000000 + $hash;
    return $color > 0x7fffffff ? (int)($color - 0x100000000) : (int)$color;
}

function map_platform(string $os): string
{
    $os = strtolower($os);
    if (strpos($os, 'win') !== false) {
        return 'Windows';
    }
    if (strpos($os, 'mac') !== false || strpos($os, 'darwin') !== false) {
        return 'Mac OS';
    }
    if (strpos($os, 'android') !== false) {
        return 'Android';
    }
    if (strpos($os, 'linux') !== false) {
        return 'Linux';
    }
    return $os;
}

class HttpError extends RuntimeException
{
    public $statusCode;

    public function __construct(string $message, int $statusCode)
    {
        parent::__construct($message);
        $this->statusCode = $statusCode;
    }
}
