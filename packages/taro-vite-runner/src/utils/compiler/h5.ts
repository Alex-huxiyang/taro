import {
  readConfig,
  resolveMainFilePath,
  resolveScriptPath,
} from '@tarojs/helper'
import { ViteH5BuildConfig, ViteH5CompilerContext } from '@tarojs/taro/types/compile/viteCompilerContext'
import path from 'path'

import { Compiler } from './base'

import type { PageConfig } from '@tarojs/taro'

export class TaroCompiler extends Compiler<ViteH5BuildConfig> implements ViteH5CompilerContext {
  routerMeta: {
    routerCreator: string
    getRoutesConfig: (pageName?: string) => string
  }

  constructor (appPath: string, taroConfig: ViteH5BuildConfig) {
    super(appPath, taroConfig)
    this.app = this.getApp()
    this.pages = this.getPages()
  }

  getAppScriptPath (): string {
    const entry = this.taroConfig.entry.app[0].replace(/\.config$/, '')
    return resolveScriptPath(entry)
  }

  compilePage = (pageName: string) => {
    const { sourceDir, frameworkExts } = this

    const scriptPath = resolveMainFilePath(path.join(sourceDir, pageName), frameworkExts)
    const configPath = this.getConfigFilePath(scriptPath)
    const config: PageConfig = readConfig(configPath) || {}

    const pageMeta = {
      name: pageName,
      scriptPath,
      configPath,
      config,
      isNative: false,
    }

    this.filesConfig[this.getConfigFilePath(pageMeta.name)] = {
      path: configPath,
      content: config
    }

    this.configFileList.push(pageMeta.configPath)
    return pageMeta
  }
}
