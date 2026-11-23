export const isMac = /Mac/i.test(navigator.userAgent);
export const osName = isMac ? "macOS" : "Windows";
export const credStore = isMac ? "macOS 钥匙串" : "Windows 凭据管理器";