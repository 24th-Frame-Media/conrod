-- Selftest.lua -- tests for Json.lua, runnable two ways:
--
--   1. Outside Lightroom (any Lua 5.1/LuaJIT on PATH):
--        lua Selftest.lua          -- prints one line per test, exit 1 on failure
--        luajit Selftest.lua
--
--   2. Inside Lightroom Classic: register MenuSelftest.lua temporarily in
--      ConrodBridge.lrplugin/Info.lua (see README.md). It shows the same
--      results in a dialog.
--
-- The tests cover the encoder/decoder contract the bridge relies on, not
-- Lightroom APIs.

local scriptDirectory = '.'
if type(arg) == 'table' and type(arg[0]) == 'string' then
  local directory = arg[0]:match('^(.*)[/\\][^/\\]+$')
  if directory and directory ~= '' then
    scriptDirectory = directory
  end
end

local Json = dofile(scriptDirectory .. '/Json.lua')

local Selftest = {}

local function deepEqual(a, b)
  if a == b then
    return true
  end
  if type(a) ~= 'table' or type(b) ~= 'table' then
    return false
  end
  for key, value in pairs(a) do
    if not deepEqual(value, b[key]) then
      return false
    end
  end
  for key in pairs(b) do
    if a[key] == nil then
      return false
    end
  end
  return true
end

-- Runs every test. Returns a list of lines and a count of failures.
function Selftest.run()
  local lines = {}
  local failures = 0
  local total = 0

  local function test(name, func)
    total = total + 1
    local ok, problem = pcall(func)
    if ok then
      lines[#lines + 1] = 'PASS  ' .. name
    else
      failures = failures + 1
      lines[#lines + 1] = 'FAIL  ' .. name .. ': ' .. tostring(problem)
    end
  end

  local function assertEqual(actual, expected, label)
    if not deepEqual(actual, expected) then
      error(string.format('%s: expected %s, got %s',
        label or 'value', tostring(expected), tostring(actual)), 2)
    end
  end

  local function assertDecodeError(text, label)
    local value, message = Json.decode(text)
    if value ~= nil then
      error((label or 'decode') .. ': expected a rejection, got a value', 2)
    end
    if type(message) ~= 'string' or message == '' then
      error((label or 'decode') .. ': no error message', 2)
    end
  end

  test('encodes scalars', function()
    assertEqual(Json.encode(true):gsub('%s+$', ''), 'true')
    assertEqual(Json.encode(false):gsub('%s+$', ''), 'false')
    assertEqual(Json.encode(Json.null):gsub('%s+$', ''), 'null')
    assertEqual(Json.encode(0):gsub('%s+$', ''), '0')
    assertEqual(Json.encode(-1):gsub('%s+$', ''), '-1')
    assertEqual(Json.encode(5):gsub('%s+$', ''), '5')
  end)

  test('encodes an empty object and an empty array distinctly', function()
    assertEqual(Json.encode({}):gsub('%s+$', ''), '{}')
    assertEqual(Json.encode(Json.array()):gsub('%s+$', ''), '[]')
  end)

  test('escapes quotes, backslashes and control characters', function()
    local encoded = Json.encode('a"b\\c\nd\te\1')
    assertEqual(encoded:gsub('%s+$', ''), '"a\\"b\\\\c\\nd\\te\\u0001"')
    local decoded = Json.decode(encoded)
    assertEqual(decoded, 'a"b\\c\nd\te\1')
  end)

  test('round-trips the bridge envelope', function()
    local envelope = {
      format = 'conrod.lightroom.bridge',
      version = 1,
      direction = 'to-conrod',
      album = { label = 'Album A', root = 'C:\\Photos\\Album-A' },
      created_at = '2026-11-19T16:00:00Z',
      producer = { app = 'conrod-bridge', version = '1.0.0' },
      photos = {
        { path = 'D:\\a\\A.CR3', rating = 5, pick = 1, label = 'green' },
        { path = 'D:\\a\\B.CR3', rating = Json.null, pick = 0, label = Json.null },
        { path = 'D:\\a\\C.CR3', rating = 2, pick = -1, label = 'red' },
      },
    }
    local decoded = Json.decode(Json.encode(envelope))
    assertEqual(decoded, envelope)
    assertEqual(decoded.photos[2].rating, Json.null)
  end)

  test('decodes unknown fields and nested arrays', function()
    local text = '{"format":"x","extra":{"a":[1,2,{"b":true}]},"photos":[]}'
    local decoded = Json.decode(text)
    assertEqual(decoded.format, 'x')
    assertEqual(decoded.extra.a[3].b, true)
    assertEqual(decoded.photos, Json.array())
  end)

  test('keeps array order and sorts object keys', function()
    local encoded = Json.encode({ z = 1, a = 2, list = { 'c', 'a', 'b' } })
    local order = {}
    for key in encoded:gmatch('"([^"]+)"%s*:') do
      order[#order + 1] = key
    end
    assertEqual(order, { 'a', 'list', 'z' })
    assertEqual(Json.decode(encoded).list[1], 'c')
    assertEqual(Json.decode(encoded).list[3], 'b')
  end)

  test('decodes numbers, exponents and negative values', function()
    local decoded = Json.decode('{"a":0,"b":-12,"c":1.5,"d":2e3,"e":-1.25e-2}')
    assertEqual(decoded.a, 0)
    assertEqual(decoded.b, -12)
    assertEqual(decoded.c, 1.5)
    assertEqual(decoded.d, 2000)
    assertEqual(decoded.e, -0.0125)
  end)

  test('decodes the six string escapes', function()
    assertEqual(Json.decode('"a\\"b\\\\c\\/d\\be\\ff\\ng\\rh\\ti"'),
      'a"b\\c/d\be\ff\ng\rh\ti')
  end)

  test('decodes \\u escapes to UTF-8', function()
    assertEqual(Json.decode('"\\u00e9"'), '\195\169')
    assertEqual(Json.decode('"\\u20ac"'), '\226\130\172')
    assertEqual(Json.decode('"\\ud83d\\ude00"'), '\240\159\152\128')
    assertEqual(Json.decode('"\\uD800"'), '\239\191\189')
  end)

  test('accepts whitespace everywhere legal', function()
    local decoded = Json.decode(' \n{\t"a" : [ 1 , 2 ] , "b" : { } } \r\n')
    assertEqual(decoded.a[2], 2)
    assertEqual(decoded.b, {})
  end)

  test('rejects malformed input with a message', function()
    assertDecodeError('', 'empty')
    assertDecodeError('{', 'unterminated object')
    assertDecodeError('{"a":1,}', 'trailing comma')
    assertDecodeError('{"a" 1}', 'missing colon')
    assertDecodeError('[1,2', 'unterminated array')
    assertDecodeError('"unterminated', 'unterminated string')
    assertDecodeError('"bad \\q escape"', 'bad escape')
    assertDecodeError('tru', 'bad literal')
    assertDecodeError('{"a":01x}', 'bad number')
    assertDecodeError('{"a":1} trailing', 'trailing characters')
    assertDecodeError('{"a":"\1"}', 'raw control character')
    assertDecodeError('{"a":}', 'missing value')
    assertDecodeError('nul', 'short null')
  end)

  test('rejects non-finite numbers on encode', function()
    local ok = pcall(function() Json.encode(0 / 0) end)
    if ok then
      error('NaN should not encode')
    end
    local okInf = pcall(function() Json.encode(math.huge) end)
    if okInf then
      error('infinity should not encode')
    end
  end)

  test('rejects unsupported table keys on encode', function()
    local ok = pcall(function() Json.encode({ [true] = 1 }) end)
    if ok then
      error('boolean key should not encode')
    end
  end)

  test('re-encodes a decoded package deterministically', function()
    local text = Json.encode({
      format = 'conrod.lightroom.bridge',
      version = 1,
      direction = 'to-lightroom',
      collection = 'Conrod keepers - Album A',
      album = { job_id = 109, label = 'Album A', root = 'C:\\Photos' },
      photos = {
        { path = 'D:\\a\\B.CR3', rating = 3, pick = 1, label = 'green',
          keywords = { 'Conrod keeper', 'Conrod burst 145' }, stars = 4,
          verdict = 'sharp', burst = 145, keeper = true },
      },
    })
    local decoded = Json.decode(text)
    if not decoded then
      error('first decode failed')
    end
    assertEqual(Json.encode(decoded), text)
  end)

  test('accepts CRLF and a UTF-8 BOM-free package from conrod', function()
    local text = '{\r\n  "format": "conrod.lightroom.bridge",\r\n  "version": 1,\r\n'
      .. '  "direction": "to-lightroom",\r\n  "collection": null,\r\n'
      .. '  "photos": [\r\n    { "path": "D:\\\\x\\\\20261006_B1000_0202.CR3", '
      .. '"rating": 5, "pick": 1, "label": "green" }\r\n  ]\r\n}\r\n'
    local decoded = Json.decode(text)
    if not decoded then
      error('CRLF package failed to decode')
    end
    assertEqual(decoded.collection, Json.null)
    assertEqual(decoded.photos[1].pick, 1)
  end)

  lines[#lines + 1] = string.format('%d tests, %d failed', total, failures)
  return lines, failures
end

-- Standalone run: Lightroom has no `arg` table, so this only fires for
-- lua/luajit from a shell.
if type(arg) == 'table' and type(rawget(_G, '_PLUGIN')) ~= 'table' then
  local lines, failures = Selftest.run()
  for _, line in ipairs(lines) do
    print(line)
  end
  if failures > 0 then
    os.exit(1)
  end
  os.exit(0)
end

return Selftest
