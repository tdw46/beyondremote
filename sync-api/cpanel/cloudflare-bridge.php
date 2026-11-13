<?php
// Temporary DNS-propagation bridge. No PostgreSQL connection or local writes.
// Install only after the native destination has passed the cutover checklist.
declare(strict_types=1);
header('Cache-Control: no-store');
header('X-Content-Type-Options: nosniff');
header('X-Robots-Tag: noindex, nofollow');
header('Referrer-Policy: no-referrer');
function bridge_error(int $status): void {
    http_response_code($status);
    header('Content-Type: application/json');
    header('Retry-After: 30');
    echo '{"error":"Service temporarily unavailable"}';
    exit;
}
if (is_file('/home/flip2t5/beyondremote-migration-fence')) bridge_error(503);
$method = $_SERVER['REQUEST_METHOD'] ?? 'GET';
if (!in_array($method, ['GET','HEAD','POST','PUT','DELETE','OPTIONS'], true)) bridge_error(405);
$uri = $_SERVER['REQUEST_URI'] ?? '/';
if (strlen($uri) > 8192 || substr($uri, 0, 1) !== '/' || preg_match('/[\r\n\x00]/', $uri)) bridge_error(400);
$input = file_get_contents('php://input', false, null, 0, 65537);
if ($input === false || strlen($input) > 65536) bridge_error(413);
$headers = [];
if (empty($_SERVER['HTTP_AUTHORIZATION']) && isset($_SERVER['REDIRECT_HTTP_AUTHORIZATION'])) {
    $_SERVER['HTTP_AUTHORIZATION'] = $_SERVER['REDIRECT_HTTP_AUTHORIZATION'];
}
foreach (['HTTP_AUTHORIZATION'=>'Authorization','CONTENT_TYPE'=>'Content-Type','HTTP_ORIGIN'=>'Origin'] as $key=>$name) {
    $value = $_SERVER[$key] ?? '';
    if (strlen($value) > 1024 || preg_match('/[\r\n]/', $value)) bridge_error(400);
    if ($value !== '') $headers[] = $name . ': ' . $value;
}
$response = '';
$responseHeaders = [];
$curl = curl_init('https://beyondremote-native-production.support-commsopen-shop.workers.dev' . $uri);
curl_setopt_array($curl, [
    CURLOPT_CUSTOMREQUEST=>$method, CURLOPT_HTTPHEADER=>$headers,
    CURLOPT_CONNECTTIMEOUT=>5, CURLOPT_TIMEOUT=>25,
    CURLOPT_FOLLOWLOCATION=>false, CURLOPT_SSL_VERIFYPEER=>true,
    CURLOPT_SSL_VERIFYHOST=>2, CURLOPT_PROTOCOLS=>CURLPROTO_HTTPS,
    CURLOPT_HEADERFUNCTION=>function ($ch, $line) use (&$responseHeaders) {
        $parts = explode(':', $line, 2);
        if (count($parts) === 2 && in_array(strtolower($parts[0]), ['content-type','location','access-control-allow-origin','access-control-allow-methods','access-control-allow-headers'], true)) {
            $responseHeaders[$parts[0]] = trim($parts[1]);
        }
        return strlen($line);
    },
    CURLOPT_WRITEFUNCTION=>function ($ch, $bytes) use (&$response) {
        if (strlen($response) + strlen($bytes) > 1048576) return 0;
        $response .= $bytes;
        return strlen($bytes);
    },
]);
if (in_array($method, ['POST','PUT','DELETE'], true)) curl_setopt($curl, CURLOPT_POSTFIELDS, $input);
if ($method === 'HEAD') curl_setopt($curl, CURLOPT_NOBODY, true);
$ok = curl_exec($curl);
$status = (int)curl_getinfo($curl, CURLINFO_HTTP_CODE);
curl_close($curl);
if ($ok === false || $status < 200 || $status > 599) bridge_error(502);
http_response_code($status);
foreach ($responseHeaders as $name=>$value) header($name . ': ' . $value);
if ($method !== 'HEAD') echo $response;
