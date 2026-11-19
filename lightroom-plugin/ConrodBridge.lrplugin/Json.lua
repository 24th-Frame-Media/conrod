-- Json.lua -- minimal JSON encoder/decoder for the Conrod Bridge plug-in.
--
-- Lightroom Classic ships no JSON module and only Lua 5.1, so this file is
-- self-contained: no imports, no external modules, no goto, no bitwise ops.
--
-- Scope: exactly the envelope in docs/plans/lightroom-bridge.md. Objects,
-- arrays, strings, numbers, true/false/null. Unknown fields are preserved by
-- the decoder. Encoding is deterministic (object keys sorted, arrays in
-- order) so package diffs stay readable.
--
-- null: Lua cannot store nil inside a table, so JSON null decodes to the
-- unique sentinel Json.null and encodes back to null. A plain empty table
-- encodes as {}, Json.array{} encodes as [].

local Json = {}

local nullValue = setmetatable({}, {
  __tostring = function() return 'null' end,
})
Json.null = nullValue

local arrayMeta = { __jsonArray = true }

-- Marks a table as a JSON array, including the empty table.
function Json.array(value)
  value = value or {}
  return setmetatable(value, arrayMeta)
end

local function isArray(value)
  if getmetatable(value) == arrayMeta then
    return true
  end
  local count = 0
  for key in pairs(value) do
    if type(key) ~= 'number' or key < 1 or key % 1 ~= 0 then
      return false
    end
    count = count + 1
  end
  if count == 0 then
    return false
  end
  for index = 1, count do
    if value[index] == nil then
      return false
    end
  end
  return true
end

local stringEscapes = {
  ['"'] = '\\"',
  ['\\'] = '\\\\',
  ['\b'] = '\\b',
  ['\f'] = '\\f',
  ['\n'] = '\\n',
  ['\r'] = '\\r',
  ['\t'] = '\\t',
}

local function encodeString(text)
  local escaped = text:gsub('[%z\1-\31"\\]', function(character)
    local replacement = stringEscapes[character]
    if replacement then
      return replacement
    end
    return string.format('\\u%04x', string.byte(character))
  end)
  return '"' .. escaped .. '"'
end

local function encodeNumber(value)
  if value ~= value or value == math.huge or value == -math.huge then
    error('cannot encode a non-finite number', 0)
  end
  if value % 1 == 0 and math.abs(value) < 1e15 then
    return string.format('%d', value)
  end
  local text = string.format('%.14g', value)
  -- string.format follows the C locale; JSON always needs a dot.
  if text:find(',', 1, true) then
    text = text:gsub(',', '.')
  end
  return text
end

local function sortedKeys(value)
  local keys = {}
  for key in pairs(value) do
    local kind = type(key)
    if kind ~= 'string' and kind ~= 'number' then
      error('cannot encode a table key of type ' .. kind, 0)
    end
    keys[#keys + 1] = key
  end
  table.sort(keys, function(a, b)
    if type(a) == type(b) then
      return a < b
    end
    return type(a) == 'number'
  end)
  return keys
end

local function writeIndent(out, depth)
  out[#out + 1] = '\n'
  out[#out + 1] = string.rep('  ', depth)
end

local encodeValue

encodeValue = function(value, depth, out)
  local kind = type(value)
  if value == nullValue or kind == 'nil' then
    out[#out + 1] = 'null'
    return
  end
  if kind == 'boolean' then
    out[#out + 1] = value and 'true' or 'false'
    return
  end
  if kind == 'number' then
    out[#out + 1] = encodeNumber(value)
    return
  end
  if kind == 'string' then
    out[#out + 1] = encodeString(value)
    return
  end
  if kind ~= 'table' then
    error('cannot encode a value of type ' .. kind, 0)
  end

  if isArray(value) then
    local count = #value
    if count == 0 then
      out[#out + 1] = '[]'
      return
    end
    out[#out + 1] = '['
    for index = 1, count do
      if index > 1 then
        out[#out + 1] = ','
      end
      writeIndent(out, depth + 1)
      encodeValue(value[index], depth + 1, out)
    end
    writeIndent(out, depth)
    out[#out + 1] = ']'
    return
  end

  local keys = sortedKeys(value)
  if #keys == 0 then
    out[#out + 1] = '{}'
    return
  end
  out[#out + 1] = '{'
  for index, key in ipairs(keys) do
    if index > 1 then
      out[#out + 1] = ','
    end
    writeIndent(out, depth + 1)
    out[#out + 1] = encodeString(tostring(key))
    out[#out + 1] = ': '
    encodeValue(value[key], depth + 1, out)
  end
  writeIndent(out, depth)
  out[#out + 1] = '}'
end

-- Encodes a Lua value as pretty-printed JSON. Throws on values JSON cannot
-- represent, so a package can never be written half-valid.
function Json.encode(value)
  local out = {}
  encodeValue(value, 0, out)
  out[#out + 1] = '\n'
  return table.concat(out)
end

local function utf8Character(code)
  if code < 0x80 then
    return string.char(code)
  end
  if code < 0x800 then
    return string.char(
      0xC0 + math.floor(code / 0x40),
      0x80 + (code % 0x40)
    )
  end
  if code < 0x10000 then
    return string.char(
      0xE0 + math.floor(code / 0x1000),
      0x80 + (math.floor(code / 0x40) % 0x40),
      0x80 + (code % 0x40)
    )
  end
  return string.char(
    0xF0 + math.floor(code / 0x40000),
    0x80 + (math.floor(code / 0x1000) % 0x40),
    0x80 + (math.floor(code / 0x40) % 0x40),
    0x80 + (code % 0x40)
  )
end

-- Decodes a JSON document. Returns value on success, or nil plus a message.
-- Unknown fields are kept; malformed input is always rejected with a reason.
function Json.decode(text)
  if type(text) ~= 'string' then
    return nil, 'input is not a string'
  end

  local length = #text
  local position = 1
  local failure = nil

  local function fail(message)
    failure = string.format('%s (at character %d)', message, position)
    error(failure, 0)
  end

  local function skipWhitespace()
    while position <= length do
      local character = text:sub(position, position)
      if character == ' ' or character == '\t'
        or character == '\n' or character == '\r' then
        position = position + 1
      else
        break
      end
    end
  end

  local function parseString()
    position = position + 1 -- opening quote
    local parts = {}
    local start = position
    while true do
      if position > length then
        fail('unterminated string')
      end
      local character = text:sub(position, position)
      if character == '"' then
        parts[#parts + 1] = text:sub(start, position - 1)
        position = position + 1
        return table.concat(parts)
      end
      if character == '\\' then
        parts[#parts + 1] = text:sub(start, position - 1)
        local escape = text:sub(position + 1, position + 1)
        if escape == '"' then
          parts[#parts + 1] = '"'
          position = position + 2
        elseif escape == '\\' then
          parts[#parts + 1] = '\\'
          position = position + 2
        elseif escape == '/' then
          parts[#parts + 1] = '/'
          position = position + 2
        elseif escape == 'b' then
          parts[#parts + 1] = '\b'
          position = position + 2
        elseif escape == 'f' then
          parts[#parts + 1] = '\f'
          position = position + 2
        elseif escape == 'n' then
          parts[#parts + 1] = '\n'
          position = position + 2
        elseif escape == 'r' then
          parts[#parts + 1] = '\r'
          position = position + 2
        elseif escape == 't' then
          parts[#parts + 1] = '\t'
          position = position + 2
        elseif escape == 'u' then
          local hex = text:sub(position + 2, position + 5)
          if #hex < 4 or hex:find('[^0-9a-fA-F]') then
            fail('bad \\u escape')
          end
          local code = tonumber(hex, 16)
          position = position + 6
          if code >= 0xD800 and code <= 0xDBFF then
            local low = nil
            if text:sub(position, position + 1) == '\\u' then
              local lowHex = text:sub(position + 2, position + 5)
              if #lowHex == 4 and not lowHex:find('[^0-9a-fA-F]') then
                low = tonumber(lowHex, 16)
              end
            end
            if low and low >= 0xDC00 and low <= 0xDFFF then
              code = 0x10000 + (code - 0xD800) * 0x400 + (low - 0xDC00)
              position = position + 6
            else
              code = 0xFFFD -- lone surrogate: keep the document, replace the code point
            end
          elseif code >= 0xDC00 and code <= 0xDFFF then
            code = 0xFFFD
          end
          parts[#parts + 1] = utf8Character(code)
        else
          fail('bad escape \\' .. tostring(escape))
        end
        start = position
      elseif string.byte(character) < 32 then
        fail('control character in string')
      else
        position = position + 1
      end
    end
  end

  local function parseNumber()
    local rest = text:sub(position)
    local literal = rest:match('^-?%d+%.?%d*[eE][-+]?%d+')
      or rest:match('^-?%d+%.?%d*')
    if not literal then
      fail('malformed number')
    end
    position = position + #literal
    local nextCharacter = text:sub(position, position)
    if nextCharacter ~= '' and not nextCharacter:find('[%s,}%]]') then
      fail('malformed number')
    end
    local number = tonumber(literal)
    if number == nil then
      fail('malformed number')
    end
    return number
  end

  local parseValue

  local function parseObject()
    local result = {}
    position = position + 1 -- opening brace
    skipWhitespace()
    if text:sub(position, position) == '}' then
      position = position + 1
      return result
    end
    while true do
      skipWhitespace()
      if text:sub(position, position) ~= '"' then
        fail('expected a string key')
      end
      local key = parseString()
      skipWhitespace()
      if text:sub(position, position) ~= ':' then
        fail("expected ':'")
      end
      position = position + 1
      skipWhitespace()
      result[key] = parseValue()
      skipWhitespace()
      local character = text:sub(position, position)
      if character == '}' then
        position = position + 1
        return result
      end
      if character ~= ',' then
        fail("expected ',' or '}'")
      end
      position = position + 1
    end
  end

  local function parseArray()
    local result = Json.array()
    position = position + 1 -- opening bracket
    skipWhitespace()
    if text:sub(position, position) == ']' then
      position = position + 1
      return result
    end
    local index = 1
    while true do
      skipWhitespace()
      result[index] = parseValue()
      index = index + 1
      skipWhitespace()
      local character = text:sub(position, position)
      if character == ']' then
        position = position + 1
        return result
      end
      if character ~= ',' then
        fail("expected ',' or ']'")
      end
      position = position + 1
    end
  end

  local depth = 0

  parseValue = function()
    depth = depth + 1
    if depth > 64 then
      fail('document is nested too deeply')
    end
    skipWhitespace()
    if position > length then
      fail('unexpected end of input')
    end
    local character = text:sub(position, position)
    local value

    if character == '{' then
      value = parseObject()
    elseif character == '[' then
      value = parseArray()
    elseif character == '"' then
      value = parseString()
    elseif character == '-' or character:find('%d') then
      value = parseNumber()
    elseif text:sub(position, position + 3) == 'true' then
      position = position + 4
      value = true
    elseif text:sub(position, position + 4) == 'false' then
      position = position + 5
      value = false
    elseif text:sub(position, position + 3) == 'null' then
      position = position + 4
      value = nullValue
    else
      fail('unexpected character ' .. string.format('%q', character))
    end

    depth = depth - 1
    return value
  end

  local ok, result = pcall(function()
    skipWhitespace()
    local value = parseValue()
    skipWhitespace()
    if position <= length then
      fail('trailing characters after the document')
    end
    return value
  end)

  if not ok then
    return nil, failure or tostring(result)
  end
  return result, nil
end

return Json
