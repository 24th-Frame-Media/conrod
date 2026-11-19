-- Conrod Bridge -- Lightroom Classic plug-in manifest.
--
-- Everything this plug-in uses is available in SDK 6.0, which is also the
-- oldest Lightroom Classic (then Photoshop Lightroom) version it supports.
-- Keeping the target at 6.0 means the same bundle runs on current Lightroom
-- Classic releases without claiming APIs this plug-in does not use.
return {
  LrSdkVersion = 6.0,
  LrSdkMinimumVersion = 6.0,

  LrToolkitIdentifier = 'com.conrod.bridge',
  LrPluginName = 'Conrod Bridge',

  VERSION = {
    major = 1,
    minor = 0,
    revision = 0,
    build = 1,
  },

  LrLibraryMenuItems = {
    {
      title = 'Send flags to Conrod',
      file = 'MenuSendFlags.lua',
    },
    {
      title = 'Apply verdicts from Conrod',
      file = 'MenuApplyVerdicts.lua',
    },
    {
      title = 'Show pending packages',
      file = 'MenuShowPending.lua',
    },
  },
}
