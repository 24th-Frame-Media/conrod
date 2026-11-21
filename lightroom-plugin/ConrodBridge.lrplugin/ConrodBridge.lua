-- ConrodBridge.lua -- implementation of the Conrod Bridge plug-in.
--
-- Two-way, file-based bridge with the conrod photo-culling app. The envelope
-- and the folder contract are defined in docs/plans/lightroom-bridge.md:
--
--   %USERPROFILE%\.conrod\bridge\in\      flags written for conrod
--   %USERPROFILE%\.conrod\bridge\out\     verdicts written by conrod
--   %USERPROFILE%\.conrod\bridge\done\    packages consumed successfully
--   %USERPROFILE%\.conrod\bridge\failed\  packages that could not be read
--
-- Rules honoured here: never delete or rename a photo, never touch develop
-- settings, never write a rating/pick/label the package does not carry, and
-- keep catalog write locks short (50 photos per write block).

local LrApplication = import 'LrApplication'
local LrBinding = import 'LrBinding'
local LrDialogs = import 'LrDialogs'
local LrFileUtils = import 'LrFileUtils'
local LrFunctionContext = import 'LrFunctionContext'
local LrPathUtils = import 'LrPathUtils'
local LrProgressScope = import 'LrProgressScope'
local LrShell = import 'LrShell'
local LrTasks = import 'LrTasks'
local LrView = import 'LrView'

local Json = dofile(LrPathUtils.child(_PLUGIN.path, 'Json.lua'))

local Bridge = {}

local FORMAT = 'conrod.lightroom.bridge'
local FORMAT_VERSION = 1
local PLUGIN_VERSION = '1.0.0'
local CHUNK_SIZE = 50
local WRITE_TIMEOUT = 60
local COLLECTION_SET_NAME = 'Conrod'
-- Lightroom dates are seconds since 2001-01-01T00:00:00Z.
local LR_EPOCH_OFFSET = 978307200

local COLORS = {
  red = true,
  yellow = true,
  green = true,
  blue = true,
  purple = true,
}

--- Small helpers -------------------------------------------------------------

local function trim(text)
  return (text:gsub('^%s+', ''):gsub('%s+$', ''))
end

-- Lua's pcall() detaches the call from Lightroom's cooperative tasking, so
-- anything that may yield (dialogs, write-access gates, progress) is called
-- through LrTasks.pcall() instead.
local function taskPcall(func, ...)
  if type(LrTasks.pcall) == 'function' then
    return LrTasks.pcall(func, ...)
  end
  return pcall(func, ...)
end

-- Absolute paths from Windows, compared case-insensitively with one
-- separator. Package paths come from conrod on Windows, so '\' is canonical.
local function normalizePath(path)
  if type(path) ~= 'string' then
    return nil
  end
  local normalized = path:gsub('/', '\\')
  normalized = normalized:gsub('%s+$', '')
  return normalized:lower()
end

local function filePart(text)
  local name = tostring(text or ''):gsub('[^%w%-%._ ]', '-')
  name = name:gsub('%s+', '-')
  name = name:gsub('%-+', '-')
  name = name:gsub('^%-+', ''):gsub('%-+$', '')
  if name == '' then
    name = 'album'
  end
  if #name > 60 then
    name = name:sub(1, 60)
  end
  return name
end

local function formatCount(count)
  return string.format('%d', count or 0)
end

local function isDirectory(path)
  local ok, kind = pcall(function() return LrFileUtils.exists(path) end)
  return ok and kind == 'directory'
end

local function exists(path)
  local ok, kind = pcall(function() return LrFileUtils.exists(path) end)
  return ok and kind ~= false
end

local function ensureDirectory(path)
  if isDirectory(path) then
    return true
  end
  pcall(function() LrFileUtils.createAllDirectories(path) end)
  if isDirectory(path) then
    return true
  end
  pcall(function() LrFileUtils.createDirectory(path) end)
  return isDirectory(path)
end

local function readText(path)
  local ok, contents = pcall(function() return LrFileUtils.readFile(path) end)
  if ok and type(contents) == 'string' then
    return contents
  end
  if type(io) ~= 'table' or type(io.open) ~= 'function' then
    return nil, 'file reading is unavailable'
  end
  local handle = io.open(path, 'rb')
  if not handle then
    return nil, 'cannot open ' .. path
  end
  local data = handle:read('*a')
  handle:close()
  if type(data) ~= 'string' then
    return nil, 'cannot read ' .. path
  end
  return data
end

-- Atomic write: <name>.json.tmp then a rename, so a reader never sees a
-- partial package. io.open is used because the SDK has no writeFile.
local function writeTextAtomic(directory, name, text)
  local finalPath = LrPathUtils.child(directory, name)
  local temporaryPath = finalPath .. '.tmp'
  if type(io) ~= 'table' or type(io.open) ~= 'function' then
    return nil, 'file writing is unavailable'
  end
  local handle, openError = io.open(temporaryPath, 'wb')
  if not handle then
    return nil, string.format('cannot write %s (%s)', temporaryPath, tostring(openError))
  end
  local wrote, writeError = pcall(function()
    handle:write(text)
  end)
  local closed, closeError = handle:close()
  if not wrote or not closed then
    pcall(function() LrFileUtils.delete(temporaryPath) end)
    return nil, string.format('write failed (%s)', tostring(writeError or closeError))
  end
  if exists(finalPath) then
    pcall(function() LrFileUtils.delete(finalPath) end)
  end
  local moved, moveResult, moveError = pcall(function()
    return LrFileUtils.move(temporaryPath, finalPath)
  end)
  if not moved or moveResult == false or not exists(finalPath) then
    pcall(function() LrFileUtils.delete(temporaryPath) end)
    return nil, string.format('rename failed (%s)', tostring(moveError or moveResult))
  end
  return finalPath
end

-- Lists a directory without ever breaking out of the LrFileUtils iterator
-- (the SDK requires the loop to run to completion).
local function listDirectory(directory)
  local entries = {}
  if not isDirectory(directory) then
    return entries
  end
  pcall(function()
    for entry in LrFileUtils.directoryEntries(directory) do
      entries[#entries + 1] = entry
    end
  end)
  local result = {}
  for _, entry in ipairs(entries) do
    local path = entry
    if not path:find('[/\\]') then
      path = LrPathUtils.child(directory, entry)
    end
    result[#result + 1] = path
  end
  return result
end

local function fileModifiedUnix(path)
  local ok, attributes = pcall(function() return LrFileUtils.fileAttributes(path) end)
  if ok and type(attributes) == 'table' and type(attributes.fileModificationDate) == 'number' then
    return attributes.fileModificationDate + LR_EPOCH_OFFSET
  end
  return nil
end

local function utcOffsetSeconds()
  local now = os.time()
  local asLocal = os.time(os.date('*t', now))
  local asUtc = os.time(os.date('!*t', now))
  if not asLocal or not asUtc then
    return 0
  end
  return os.difftime(asLocal, asUtc)
end

local function isoToUnix(text)
  if type(text) ~= 'string' then
    return nil
  end
  local y, month, day, hour, minute, second =
    text:match('^(%d%d%d%d)%-(%d%d)%-(%d%d)T(%d%d):(%d%d):(%d%d)')
  if not y then
    return nil
  end
  local parts = {
    year = tonumber(y),
    month = tonumber(month),
    day = tonumber(day),
    hour = tonumber(hour),
    min = tonumber(minute),
    sec = tonumber(second),
    isdst = false,
  }
  local asLocal = os.time(parts)
  if not asLocal then
    return nil
  end
  return asLocal + utcOffsetSeconds()
end

local function humanAge(seconds)
  if not seconds or seconds < 0 then
    return 'age unknown'
  end
  if seconds < 90 then
    return 'just now'
  end
  local minutes = math.floor(seconds / 60)
  if minutes < 60 then
    return string.format('%d min ago', minutes)
  end
  local hours = math.floor(minutes / 60)
  if hours < 48 then
    return string.format('%d h ago', hours)
  end
  return string.format('%d d ago', math.floor(hours / 24))
end

local function startProgress(context, title)
  local ok, scope = taskPcall(function()
    return LrProgressScope({
      title = title,
      functionContext = context,
    })
  end)
  if ok and type(scope) == 'table' then
    return scope
  end
  return {
    isCanceled = function() return false end,
    setPortionComplete = function() end,
    setCaption = function() end,
    setIndeterminate = function() end,
    done = function() end,
  }
end

--- Locations ----------------------------------------------------------------

local function catalogHome(catalog)
  local home = os.getenv('USERPROFILE')
  if not home or home == '' then
    home = os.getenv('HOME')
  end
  if not home or home == '' then
    local drive, rest = os.getenv('HOMEDRIVE'), os.getenv('HOMEPATH')
    if drive and rest then
      home = drive .. rest
    end
  end
  if not home or home == '' then
    local ok, value = pcall(function() return LrPathUtils.getStandardFilePath('home') end)
    if ok and type(value) == 'string' and value ~= '' then
      home = value
    end
  end
  if not home or home == '' then
    -- Last resort: walk up from the catalog looking for the app's data root.
    local ok, catalogPath = pcall(function() return catalog:getPath() end)
    if ok and type(catalogPath) == 'string' then
      local directory = LrPathUtils.parent(catalogPath)
      while directory do
        if isDirectory(LrPathUtils.child(directory, '.conrod')) then
          home = directory
          break
        end
        local parent = LrPathUtils.parent(directory)
        if not parent or parent == directory then
          break
        end
        directory = parent
      end
      if not home then
        home = LrPathUtils.parent(catalogPath)
      end
    end
  end
  return home
end

local function bridgeRoot(catalog)
  local home = catalogHome(catalog)
  if not home then
    error('cannot find the user profile folder (USERPROFILE is not set)')
  end
  return LrPathUtils.child(LrPathUtils.child(home, '.conrod'), 'bridge')
end

local FOLDER_NAMES = { 'in', 'out', 'done', 'failed' }

local function bridgeFolders(root)
  local folders = {}
  for _, name in ipairs(FOLDER_NAMES) do
    folders[name] = LrPathUtils.child(root, name)
  end
  return folders
end

--- Metadata -----------------------------------------------------------------

-- Batch read with per-photo fallback: a key some Lightroom builds refuse
-- never breaks the whole read, and an unset value is not mistaken for a
-- refused key.
local function metadataReader(catalog, photos, keys)
  local supported = {}
  local probe = photos[1]
  for _, key in ipairs(keys) do
    local ok = pcall(function() return probe:getRawMetadata(key) end)
    supported[key] = ok
  end

  local metadata = nil
  local okBatch = pcall(function()
    metadata = catalog:batchGetRawMetadata(photos, keys)
  end)
  if not okBatch or type(metadata) ~= 'table' then
    metadata = nil
    local accepted = {}
    for _, key in ipairs(keys) do
      if supported[key] then
        accepted[#accepted + 1] = key
      end
    end
    if #accepted > 0 then
      pcall(function()
        metadata = catalog:batchGetRawMetadata(photos, accepted)
      end)
    end
  end

  return function(photo, key)
    if metadata then
      local row = metadata[photo]
      if type(row) == 'table' and supported[key] then
        return row[key]
      end
    end
    local ok, value = pcall(function() return photo:getRawMetadata(key) end)
    if ok then
      return value
    end
    return nil
  end
end

local function toRating(raw)
  local value = tonumber(raw)
  if value == nil then
    return nil
  end
  value = math.floor(value + 0.5)
  if value < 0 then
    value = 0
  elseif value > 5 then
    value = 5
  end
  return value
end

local function toPick(raw)
  local value = tonumber(raw)
  if value == nil then
    return nil
  end
  if value < 0 then
    return -1
  elseif value > 0 then
    return 1
  end
  return 0
end

local function toLabel(raw)
  if type(raw) ~= 'string' then
    return nil
  end
  local name = raw:lower()
  -- 'grey' (and an empty string) is how Lightroom reports "no label".
  if name == '' or name == 'none' or name == 'grey' then
    return nil
  end
  if COLORS[name] then
    return name
  end
  return nil
end

--- Sending flags ------------------------------------------------------------

local function activeSource(catalog)
  local ok, sources = pcall(function() return catalog:getActiveSources() end)
  if not ok or type(sources) ~= 'table' or #sources == 0 then
    return nil, nil
  end
  local source = sources[1]
  if type(source) ~= 'table' then
    return nil, nil
  end
  local name = nil
  local okName, value = pcall(function() return source:getName() end)
  if okName and type(value) == 'string' and value ~= '' then
    name = value
  end
  local folderPath = nil
  local okPath, path = pcall(function() return source:getPath() end)
  if okPath and type(path) == 'string' and path ~= '' then
    folderPath = path
  end
  return name, folderPath
end

local function commonParent(paths)
  local common = nil
  for _, path in ipairs(paths) do
    local parent = LrPathUtils.parent(path)
    if not parent then
      return nil
    end
    if common == nil then
      common = parent
    elseif normalizePath(common) ~= normalizePath(parent) then
      return nil
    end
  end
  return common
end

local function albumFor(catalog, entries)
  local label, folderPath = activeSource(catalog)
  local paths = {}
  for _, entry in ipairs(entries) do
    paths[#paths + 1] = entry.path
  end
  local common = commonParent(paths)
  local root = folderPath or common
  if not label then
    if root then
      label = LrPathUtils.leafName(root)
    end
  end
  if not label or label == '' then
    label = 'Lightroom'
  end
  if not root then
    root = ''
  end
  return label, root
end

local function buildPackage(label, root, entries)
  local photos = {}
  for index, entry in ipairs(entries) do
    photos[index] = {
      path = entry.path,
      rating = entry.rating or Json.null,
      pick = entry.pick or Json.null,
      label = entry.label or Json.null,
    }
  end
  return {
    format = FORMAT,
    version = FORMAT_VERSION,
    direction = 'to-conrod',
    album = {
      label = label,
      root = (root ~= '' and root) or nil,
    },
    created_at = os.date('!%Y-%m-%dT%H:%M:%SZ'),
    producer = {
      app = 'conrod-bridge',
      version = PLUGIN_VERSION,
    },
    photos = photos,
  }
end

local function sendFlags(context)
  local catalog = LrApplication.activeCatalog()
  local photos = catalog:getTargetPhotos()
  if type(photos) ~= 'table' or #photos == 0 then
    LrDialogs.message('Conrod Bridge',
      'No photos to send. Select photos in the Library, or open a folder or collection.',
      'warning')
    return
  end

  local read = metadataReader(catalog, photos, {
    'path', 'rating', 'pickStatus', 'colorNameForLabel', 'fileName',
    'dateTimeOriginal', 'isVirtualCopy',
  })

  local progress = startProgress(context, 'Reading Lightroom flags')
  progress:setCaption(string.format('%d photos', #photos))

  -- One entry per file: when a virtual copy and its master are both selected,
  -- keep the master (same rule the apply direction uses). Duplicate paths
  -- would make conrod's path matching ambiguous.
  local byPath, order = {}, {}
  local skipped = 0
  for index, photo in ipairs(photos) do
    local path = read(photo, 'path')
    if type(path) == 'string' and path ~= '' then
      local key = normalizePath(path)
      local existing = byPath[key]
      local isCopy = read(photo, 'isVirtualCopy') and true or false
      if not existing then
        byPath[key] = {
          isCopy = isCopy,
          entry = {
            path = path,
            rating = toRating(read(photo, 'rating')),
            pick = toPick(read(photo, 'pickStatus')),
            label = toLabel(read(photo, 'colorNameForLabel')),
          },
        }
        order[#order + 1] = key
      elseif existing.isCopy and not isCopy then
        existing.isCopy = false
        existing.entry = {
          path = path,
          rating = toRating(read(photo, 'rating')),
          pick = toPick(read(photo, 'pickStatus')),
          label = toLabel(read(photo, 'colorNameForLabel')),
        }
      end
    else
      skipped = skipped + 1
    end
    if index % 200 == 0 then
      progress:setPortionComplete(index, #photos)
      LrTasks.yield()
    end
  end
  progress:done()

  local entries = {}
  for _, key in ipairs(order) do
    entries[#entries + 1] = byPath[key].entry
  end

  if #entries == 0 then
    LrDialogs.message('Conrod Bridge',
      'None of the selected photos exposes a file path, so there is nothing to send.',
      'warning')
    return
  end

  -- Deterministic package content: sort by normalised path.
  table.sort(entries, function(a, b)
    return normalizePath(a.path) < normalizePath(b.path)
  end)

  local label, root = albumFor(catalog, entries)
  local envelope = buildPackage(label, root, entries)
  local text = Json.encode(envelope)

  local folders = bridgeFolders(bridgeRoot(catalog))
  if not ensureDirectory(folders['in']) then
    error('cannot create ' .. folders['in'])
  end
  -- Unique file name: two sends in the same second must not overwrite one.
  local stamp = os.time()
  local base = string.format('%s-%d', filePart(label), stamp)
  local fileName = base .. '.json'
  local counter = 1
  while exists(LrPathUtils.child(folders['in'], fileName)) do
    counter = counter + 1
    fileName = string.format('%s-%d.json', base, counter)
  end
  local written, writeError = writeTextAtomic(folders['in'], fileName, text)
  if not written then
    error(writeError or 'the package could not be written')
  end

  local picked, rejected, rated = 0, 0, 0
  for _, entry in ipairs(entries) do
    if entry.pick == 1 then
      picked = picked + 1
    elseif entry.pick == -1 then
      rejected = rejected + 1
    end
    if entry.rating and entry.rating > 0 then
      rated = rated + 1
    end
  end

  local lines = {
    string.format('Wrote %d flags (%d picked, %d rejected, %d rated)',
      #entries, picked, rejected, rated),
    '',
    string.format('Album: %s', label),
    string.format('Folder: %s', root ~= '' and root or '(mixed)'),
    string.format('File: %s', written),
  }
  if skipped > 0 then
    lines[#lines + 1] = string.format('Skipped %d photos without a file path.', skipped)
  end
  LrDialogs.message('Conrod Bridge', table.concat(lines, '\n'), 'info')
end

--- Packets from conrod ------------------------------------------------------

local function parsePackage(path)
  local info = { path = path, name = LrPathUtils.leafName(path) }
  local text, readError = readText(path)
  if not text then
    info.error = readError or 'the file could not be read'
    info.created = fileModifiedUnix(path) or 0
    return info
  end
  local data, decodeError = Json.decode(text)
  if type(data) ~= 'table' then
    info.error = decodeError or 'the file is not a JSON object'
    info.created = fileModifiedUnix(path) or 0
    return info
  end
  info.data = data
  info.created = isoToUnix(data.created_at) or fileModifiedUnix(path) or 0
  if type(data.album) == 'table' and type(data.album.label) == 'string' then
    info.label = data.album.label
  end
  if type(data.photos) == 'table' then
    info.count = #data.photos
  end
  return info
end

local function listPackages(folders)
  local packages = {}
  for _, path in ipairs(listDirectory(folders['out'])) do
    if LrPathUtils.leafName(path):lower():match('%.json$') then
      packages[#packages + 1] = parsePackage(path)
    end
  end
  table.sort(packages, function(a, b)
    if a.created ~= b.created then
      return a.created > b.created
    end
    return a.name < b.name
  end)
  return packages
end

local function packageAge(package)
  if not package.created or package.created <= 0 then
    return 'age unknown'
  end
  return humanAge(os.time() - package.created)
end

local function packageDescription(package)
  local label = package.label or package.name
  if package.error and not package.data then
    return string.format('%s -- unreadable -- %s', label, packageAge(package))
  end
  return string.format('%s -- %s photos -- %s',
    label, formatCount(package.count or 0), packageAge(package))
end

local function validatePackage(data)
  if type(data) ~= 'table' then
    return nil, 'the package is not a JSON object'
  end
  if data.format ~= FORMAT then
    return nil, string.format('unknown format %q (expected %q)',
      tostring(data.format), FORMAT)
  end
  local version = data.version
  if type(version) ~= 'number' or version % 1 ~= 0 then
    return nil, string.format('version %s is missing or not an integer', tostring(version))
  end
  if version > FORMAT_VERSION then
    return nil, string.format('package version %d is newer than this plug-in supports (%d)',
      version, FORMAT_VERSION)
  end
  if version < 1 then
    return nil, string.format('package version %d is not supported', version)
  end
  if data.direction ~= 'to-lightroom' then
    return nil, string.format('direction is %q, expected "to-lightroom"',
      tostring(data.direction))
  end
  if type(data.photos) ~= 'table' then
    return nil, 'photos is missing or not an array'
  end
  return true
end

local function packageCollection(data)
  local name = data.collection
  if type(name) ~= 'string' then
    return nil
  end
  name = trim(name)
  if name == '' then
    return nil
  end
  return name
end

-- Masters only: a virtual copy shares its master's path, and a verdict written
-- to the master is the one the photographer sees in the Library.
local function buildPhotoMap(catalog, progress)
  local photos = catalog:getAllPhotos()
  local byPath = {}
  if type(photos) ~= 'table' then
    return byPath
  end
  local read = metadataReader(catalog, photos, { 'path', 'isVirtualCopy' })
  for index, photo in ipairs(photos) do
    local ok, isCopy = pcall(function() return read(photo, 'isVirtualCopy') end)
    if not ok or not isCopy then
      local path = read(photo, 'path')
      if type(path) == 'string' and path ~= '' then
        local key = normalizePath(path)
        if byPath[key] == nil then
          byPath[key] = photo
        end
      end
    end
    if index % 1000 == 0 then
      if progress then
        if progress:isCanceled() then
          return byPath
        end
        progress:setCaption(string.format('Reading catalog paths (%d)', index))
      end
      LrTasks.yield()
    end
  end
  return byPath
end

local function keywordsOf(entry)
  local keywords = entry.keywords
  if keywords == nil or keywords == Json.null then
    return nil, nil
  end
  if type(keywords) ~= 'table' then
    return nil, 'keywords is not an array'
  end
  local names = {}
  for _, value in ipairs(keywords) do
    if type(value) ~= 'string' then
      return nil, 'keywords contains a non-string'
    end
    local name = trim(value)
    if name ~= '' then
      names[#names + 1] = name
    end
  end
  return names, nil
end

local function classifyEntry(entry, byPath)
  local kind = type(entry)
  if kind ~= 'table' then
    return { status = 'skipped', reason = 'entry is not an object' }
  end
  local path = entry.path
  if type(path) ~= 'string' or path == '' then
    return { status = 'skipped', reason = 'entry has no path' }
  end

  local rating = entry.rating
  if rating == Json.null then
    rating = nil
  end
  if rating ~= nil then
    if type(rating) ~= 'number' or rating % 1 ~= 0 or rating < 0 or rating > 5 then
      return { status = 'skipped', path = path, reason = 'rating is not an integer 0-5' }
    end
  end

  local pick = entry.pick
  if pick == Json.null then
    pick = nil
  end
  if pick ~= nil then
    if type(pick) ~= 'number' or (pick ~= -1 and pick ~= 0 and pick ~= 1) then
      return { status = 'skipped', path = path, reason = 'pick is not -1, 0 or 1' }
    end
  end

  local label = entry.label
  if label == Json.null then
    label = nil
  end
  if label ~= nil then
    if type(label) ~= 'string' or not COLORS[label:lower()] then
      return { status = 'skipped', path = path, reason = 'label is not a colour name' }
    end
    label = label:lower()
  end

  local keywords, keywordProblem = keywordsOf(entry)
  if keywordProblem then
    return { status = 'skipped', path = path, reason = keywordProblem }
  end

  local photo = byPath[normalizePath(path)]
  if not photo then
    return { status = 'missing', path = path }
  end

  local hasVerdict = rating ~= nil or pick ~= nil or label ~= nil
    or (keywords and #keywords > 0)
  if not hasVerdict then
    return { status = 'skipped', path = path, reason = 'no verdict' }
  end

  return {
    status = 'matched',
    path = path,
    photo = photo,
    rating = rating,
    pick = pick,
    label = label,
    keywords = keywords,
  }
end

local function classifyEntries(entries, byPath)
  local matched, missing, skipped = {}, {}, {}
  for _, entry in ipairs(entries) do
    local result = classifyEntry(entry, byPath)
    if result.status == 'matched' then
      matched[#matched + 1] = result
    elseif result.status == 'missing' then
      missing[#missing + 1] = result
    else
      skipped[#skipped + 1] = result
    end
  end
  return matched, missing, skipped
end

local function findOrCreateCollectionSet(catalog)
  local ok, sets = pcall(function() return catalog:getChildCollectionSets() end)
  if ok and type(sets) == 'table' then
    for _, candidate in ipairs(sets) do
      local okName, name = pcall(function() return candidate:getName() end)
      if okName and name == COLLECTION_SET_NAME then
        return candidate
      end
    end
  end
  local created, result = taskPcall(function()
    return catalog:createCollectionSet(COLLECTION_SET_NAME, nil, true)
  end)
  if created then
    return result
  end
  return nil
end

local function findOrCreateCollection(catalog, name)
  local set = findOrCreateCollectionSet(catalog)
  if not set then
    return nil
  end
  local ok, children = pcall(function() return set:getChildCollections() end)
  if ok and type(children) == 'table' then
    for _, candidate in ipairs(children) do
      local okName, childName = pcall(function() return candidate:getName() end)
      if okName and childName == name then
        return candidate
      end
    end
  end
  local created, result = taskPcall(function()
    return catalog:createCollection(name, set, true)
  end)
  if created then
    return result
  end
  return nil
end

local function keywordFor(catalog, name, cache)
  if cache[name] ~= nil then
    return cache[name]
  end
  local ok, keyword = taskPcall(function()
    return catalog:createKeyword(name, {}, true, nil, true)
  end)
  if not ok or not keyword then
    taskPcall(function()
      keyword = catalog:createKeyword(name, {}, true, nil, false)
    end)
  end
  if keyword then
    cache[name] = keyword
  end
  return keyword
end

local function setLabel(photo, name)
  -- 'colorNameForLabel' is the documented writable key; 'label' is the
  -- formatted-metadata name for the same value and is kept as a fallback for
  -- builds that only accept that spelling.
  local ok = pcall(function() photo:setRawMetadata('colorNameForLabel', name) end)
  if ok then
    return true
  end
  return pcall(function() photo:setRawMetadata('label', name) end)
end

-- Applies one chunk inside a single write block. Per-photo errors are caught
-- and counted, so one bad entry never aborts the package.
--
-- Conrod never clears a choice the photographer made by hand: a rating or
-- colour label is only written when the photo has none, unless the package
-- says to overwrite, and a pick flag is only set on an unflagged photo (a
-- flag is never cleared, and a rejection never replaces a hand pick).
local function applyChunk(catalog, chunk, collectionName, keywordCache, result, overwrite)
  overwrite = overwrite or {}
  local function current(photo, key)
    local ok, value = pcall(function() return photo:getRawMetadata(key) end)
    if ok then
      return value
    end
    return nil
  end
  catalog:withWriteAccessDo('Conrod verdicts', function()
    for _, item in ipairs(chunk) do
      local okPhoto, problem = pcall(function()
        local photo = item.photo
        if item.rating ~= nil
          and (overwrite.rating or (tonumber(current(photo, 'rating')) or 0) == 0)
        then
          photo:setRawMetadata('rating', item.rating)
        end
        if item.pick ~= nil and item.pick ~= 0 then
          local pick = toPick(current(photo, 'pickStatus')) or 0
          if pick == 0 then
            photo:setRawMetadata('pickStatus', item.pick)
          end
        end
        if item.label ~= nil
          and (overwrite.label or toLabel(current(photo, 'colorNameForLabel')) == nil)
        then
          if not setLabel(photo, item.label) then
            error('the colour label could not be set')
          end
        end
        for _, name in ipairs(item.keywords or {}) do
          local keyword = keywordFor(catalog, name, keywordCache)
          if keyword then
            photo:addKeyword(keyword)
          end
        end
      end)
      if okPhoto then
        result.applied = result.applied + 1
      else
        result.failed = result.failed + 1
        result.problems[#result.problems + 1] =
          string.format('%s: %s', LrPathUtils.leafName(item.path), tostring(problem))
      end
    end

    if collectionName then
      local picked = {}
      for _, item in ipairs(chunk) do
        if item.pick == 1 then
          picked[#picked + 1] = item.photo
        end
      end
      if #picked > 0 then
        local collection = findOrCreateCollection(catalog, collectionName)
        if collection then
          collection:addPhotos(picked)
          result.addedToCollection = result.addedToCollection + #picked
        else
          result.collectionError =
            string.format('could not create the collection %q', collectionName)
        end
      end
    end
  end, { timeout = WRITE_TIMEOUT })
end

local function applyMatched(catalog, matched, collectionName, progress, overwrite)
  local result = {
    applied = 0,
    failed = 0,
    addedToCollection = 0,
    problems = {},
    stopped = nil,
    collectionError = nil,
  }
  local keywordCache = {}
  local index = 1
  while index <= #matched do
    if progress:isCanceled() then
      result.stopped = 'canceled'
      return result
    end
    local last = math.min(index + CHUNK_SIZE - 1, #matched)
    local chunk = {}
    for position = index, last do
      chunk[#chunk + 1] = matched[position]
    end

    local ok, problem = taskPcall(function()
      applyChunk(catalog, chunk, collectionName, keywordCache, result, overwrite)
    end)
    if not ok then
      result.stopped = string.format('write access failed: %s', tostring(problem))
      return result
    end

    index = last + 1
    progress:setPortionComplete(index - 1, #matched)
    progress:setCaption(string.format('%d of %d photos', math.min(index - 1, #matched), #matched))
    LrTasks.yield()
  end
  return result
end

local function moveToFolder(package, folders, folderName, suffix)
  local directory = folders[folderName]
  if not ensureDirectory(directory) then
    return nil, 'cannot create ' .. directory
  end
  local base = package.name:gsub('%.[jJ][sS][oO][nN]$', '')
  local stamp = os.time()
  local target = LrPathUtils.child(directory, string.format('%s.%d%s', base, stamp, suffix))
  local counter = 1
  while exists(target) do
    counter = counter + 1
    target = LrPathUtils.child(directory,
      string.format('%s.%d-%d%s', base, stamp, counter, suffix))
  end
  local ok, moved, problem = pcall(function() return LrFileUtils.move(package.path, target) end)
  if ok and moved ~= false and exists(target) then
    return target
  end
  return nil, tostring(problem or moved or 'the file could not be moved')
end

local function failPackage(package, folders, reason)
  local target, problem = moveToFolder(package, folders, 'failed', '.failed')
  local lines = {
    string.format('This package cannot be applied:'),
    '',
    reason,
    '',
  }
  if target then
    lines[#lines + 1] = 'Moved to:'
    lines[#lines + 1] = target
  else
    lines[#lines + 1] = 'It could not be moved to the failed folder:'
    lines[#lines + 1] = tostring(problem)
  end
  LrDialogs.message('Conrod Bridge', table.concat(lines, '\n'), 'warning')
end

local function choosePackage(context, packages)
  local newest = packages[1]
  local ok, choice, checkOnly = taskPcall(function()
    local properties = LrBinding.makePropertyTable(context)
    properties.choice = newest.path
    local factory = LrView.osFactory()
    local items = {}
    for _, package in ipairs(packages) do
      items[#items + 1] = {
        title = packageDescription(package),
        value = package.path,
      }
    end
    local contents = factory:column {
      bind_to_object = properties,
      spacing = factory:control_spacing(),
      factory:static_text {
        title = 'Choose a package from conrod (newest first):',
      },
      factory:popup_menu {
        value = LrView.bind('choice'),
        items = items,
        width = 460,
      },
      factory:static_text {
        title = '"Check only" validates and reports without changing the catalog.',
        font = '<system/small>',
      },
    }
    local answer = LrDialogs.presentModalDialog({
      title = 'Apply verdicts from Conrod',
      contents = contents,
      actionVerb = 'Apply',
      cancelVerb = 'Cancel',
      otherVerb = 'Check only',
    })
    if answer ~= 'ok' and answer ~= 'other' then
      return nil, false
    end
    local selected = nil
    for _, package in ipairs(packages) do
      if package.path == properties.choice then
        selected = package
        break
      end
    end
    return selected or newest, answer == 'other'
  end)

  if not ok or choice == nil then
    -- Either the view could not be built or the user canceled. On a view
    -- failure, offer the newest package instead of failing silently.
    if not ok then
      if LrDialogs.confirm(
          string.format('Use the newest package?\n\n%s', packageDescription(newest)),
          'The package chooser could not be shown.',
          'Use newest', 'Cancel') ~= 'ok' then
        return nil, false
      end
      return newest, false
    end
    return nil, false
  end
  return choice, checkOnly
end

local function preflightText(package, matched, missing, skipped, collectionName)
  local lines = {
    string.format('Package: %s', package.name),
    string.format('Album:   %s', package.label or '(no album label)'),
    string.format('Age:     %s', packageAge(package)),
    '',
    string.format('Matched:               %d', #matched),
    string.format('Not in this catalog:   %d', #missing),
    string.format('Skipped (no verdict):  %d', #skipped),
  }
  if collectionName then
    lines[#lines + 1] = ''
    lines[#lines + 1] = string.format('Collection: %s (picked photos only)', collectionName)
  end
  return table.concat(lines, '\n')
end

local function detailText(matched, missing, skipped)
  local lines = {}
  local function addList(title, items, describe, limit)
    if #items == 0 then
      return
    end
    lines[#lines + 1] = ''
    lines[#lines + 1] = title
    for index = 1, math.min(#items, limit or 10) do
      lines[#lines + 1] = '  ' .. describe(items[index])
    end
    if #items > (limit or 10) then
      lines[#lines + 1] = string.format('  ... and %d more', #items - (limit or 10))
    end
  end
  addList('Matched:', matched, function(item)
    return LrPathUtils.leafName(item.path)
  end)
  addList('Not in this catalog:', missing, function(item)
    return item.path
  end, 5)
  addList('Skipped:', skipped, function(item)
    local name = item.path and LrPathUtils.leafName(item.path) or '?'
    return string.format('%s (%s)', name, item.reason or 'no verdict')
  end, 5)
  return table.concat(lines, '\n')
end

local function applyVerdicts(context)
  local catalog = LrApplication.activeCatalog()
  local folders = bridgeFolders(bridgeRoot(catalog))

  if not isDirectory(folders['out']) then
    LrDialogs.message('Conrod Bridge',
      string.format('No packages yet.\n\nThe bridge folder is:\n%s', folders['out']),
      'info')
    return
  end

  local packages = listPackages(folders)
  if #packages == 0 then
    LrDialogs.message('Conrod Bridge',
      string.format('No pending packages in\n%s', folders['out']),
      'info')
    return
  end

  local package, checkOnly = choosePackage(context, packages)
  if not package then
    return
  end

  if type(package.data) ~= 'table' then
    failPackage(package, folders, package.error or 'the package could not be read')
    return
  end

  local valid, reason = validatePackage(package.data)
  if not valid then
    failPackage(package, folders, reason)
    return
  end

  local collectionName = packageCollection(package.data)

  local progress = startProgress(context, 'Conrod verdicts')
  progress:setCaption('Reading catalog paths')
  progress:setIndeterminate()
  local byPath = buildPhotoMap(catalog, progress)
  local canceled = progress:isCanceled()
  progress:done()
  if canceled then
    return
  end

  local matched, missing, skipped =
    classifyEntries(package.data.photos, byPath)

  local summary = preflightText(package, matched, missing, skipped, collectionName)
  local details = detailText(matched, missing, skipped)

  if checkOnly then
    LrDialogs.message('Conrod Bridge',
      table.concat({
        'Check only: nothing was changed.',
        '',
        summary,
        details,
      }, '\n'),
      'info')
    return
  end

  if #matched == 0 then
    LrDialogs.message('Conrod Bridge',
      table.concat({
        'Nothing to apply.',
        '',
        summary,
        details,
        '',
        'The package was left in:',
        folders['out'],
      }, '\n'),
      'warning')
    return
  end

  if LrDialogs.confirm('Conrod Bridge',
      summary .. '\n\nThe package is applied in chunks of ' .. CHUNK_SIZE
        .. ' photos, then moved to the done folder.',
      'Apply', 'Cancel') ~= 'ok' then
    return
  end

  local applyProgress = startProgress(context, 'Applying Conrod verdicts')
  applyProgress:setPortionComplete(0, #matched)
  local overwrite = package.data.overwrite
  if type(overwrite) ~= 'table' then
    overwrite = {}
  end
  local result = applyMatched(catalog, matched, collectionName, applyProgress, {
    rating = overwrite.rating == true,
    label = overwrite.label == true,
  })
  applyProgress:done()

  local lines = {
    string.format('Applied %d photos.', result.applied),
  }
  if result.failed > 0 then
    lines[1] = string.format('Applied %d photos, %d failed.', result.applied, result.failed)
  end
  if collectionName then
    if result.addedToCollection > 0 then
      lines[#lines + 1] = string.format('Added %d picked photos to "%s".',
        result.addedToCollection, collectionName)
    elseif result.collectionError then
      lines[#lines + 1] = result.collectionError
    end
  end
  if #missing > 0 then
    lines[#lines + 1] = string.format('%d package photos are not in this catalog.', #missing)
  end
  if #skipped > 0 then
    lines[#lines + 1] = string.format('%d entries were skipped (no verdict or invalid).', #skipped)
  end
  if #result.problems > 0 then
    lines[#lines + 1] = ''
    lines[#lines + 1] = 'Failed photos:'
    for index = 1, math.min(#result.problems, 8) do
      lines[#lines + 1] = '  ' .. result.problems[index]
    end
    if #result.problems > 8 then
      lines[#lines + 1] = string.format('  ... and %d more', #result.problems - 8)
    end
  end

  if result.stopped then
    lines[#lines + 1] = ''
    lines[#lines + 1] = 'Stopped: ' .. result.stopped
    lines[#lines + 1] = 'The package is still in:'
    lines[#lines + 1] = folders['out']
    lines[#lines + 1] = 'Re-running it is safe: already-applied photos are idempotent.'
    LrDialogs.message('Conrod Bridge', table.concat(lines, '\n'), 'warning')
    return
  end

  local target, moveProblem = moveToFolder(package, folders, 'done', '.done')
  if target then
    lines[#lines + 1] = ''
    lines[#lines + 1] = 'Moved to:'
    lines[#lines + 1] = target
  else
    lines[#lines + 1] = ''
    lines[#lines + 1] = 'The package was applied but could not be moved to the done folder:'
    lines[#lines + 1] = tostring(moveProblem)
  end
  LrDialogs.message('Conrod Bridge', table.concat(lines, '\n'), 'info')
end

--- Pending packages ---------------------------------------------------------

local function showPending(context)
  local catalog = LrApplication.activeCatalog()
  local folders = bridgeFolders(bridgeRoot(catalog))
  local lines = {}

  if not ensureDirectory(folders['out']) then
    LrDialogs.message('Conrod Bridge',
      string.format('Cannot create the bridge folder:\n%s', folders['out']),
      'critical')
    return
  end

  local packages = listPackages(folders)

  if #packages == 0 then
    lines[#lines + 1] = 'No pending packages.'
  else
    lines[#lines + 1] = string.format('%d pending package%s:',
      #packages, #packages == 1 and '' or 's')
    lines[#lines + 1] = ''
    for index, package in ipairs(packages) do
      lines[#lines + 1] = string.format('%d. %s', index, packageDescription(package))
      lines[#lines + 1] = '   ' .. LrPathUtils.leafName(package.path)
    end
  end
  lines[#lines + 1] = ''
  lines[#lines + 1] = 'Folder:'
  lines[#lines + 1] = folders['out']

  LrDialogs.message('Conrod Bridge', table.concat(lines, '\n'), 'info')
  pcall(function() LrShell.revealInShell(folders['out']) end)
end

--- Menu entry points --------------------------------------------------------

local function runAsync(name, action)
  LrTasks.startAsyncTask(function()
    LrFunctionContext.callWithContext(name, function(context)
      local ok, problem = taskPcall(action, context)
      if not ok then
        LrDialogs.message('Conrod Bridge',
          string.format('%s failed:\n\n%s', name, tostring(problem)),
          'critical')
      end
    end)
  end, name)
end

function Bridge.sendFlags()
  runAsync('Send flags to Conrod', sendFlags)
end

function Bridge.applyVerdicts()
  runAsync('Apply verdicts from Conrod', applyVerdicts)
end

function Bridge.showPending()
  runAsync('Show pending packages', showPending)
end

return Bridge
