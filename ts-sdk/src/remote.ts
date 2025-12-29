/**
 * リモートDB操作用のSSH接続ラッパー
 */
import { Client, ConnectConfig } from 'ssh2';
import SSHConfig from 'ssh-config';
import { readFileSync } from 'fs';
import { homedir } from 'os';
import { resolve } from 'path';
import type {
  Media,
  MediaInput,
  MediaFilter,
  QueryOptions,
  Tag,
  MediaAttribute,
  TableColumnInfo,
} from './types.js';

/**
 * リモート接続設定
 */
export interface RemoteConfig {
  sshHost: string;           // .ssh/configのHost名（必須）
  dbPath?: string;           // リモートのDBパス（デフォルト: ~/.local/share/kijuku/kijuku.db）
  workDir?: string;          // 作業ディレクトリ（省略可、将来の拡張用）
  binaryPath?: string;       // バイナリパス（デフォルト: ~/.local/bin/kijuku-cli）
  port?: number;             // SSHポート（省略時はSSH設定から読み取り）
}

/**
 * コマンドリクエスト
 */
interface CommandRequest {
  operation: string;
  params: any;
}

/**
 * コマンドレスポンス
 */
interface CommandResponse {
  success: boolean;
  data?: any;
  error?: string;
}

/**
 * リモートKijuku DB操作クラス
 */
export class RemoteKijukuDB {
  private sshClient: Client | null = null;
  private config: RemoteConfig;

  constructor(config: RemoteConfig) {
    this.config = config;
  }

  /**
   * SSH設定を読み込む
   */
  private loadSSHConfig(): ConnectConfig {
    const configPath = resolve(homedir(), '.ssh', 'config');
    const configContent = readFileSync(configPath, 'utf-8');
    const config = SSHConfig.parse(configContent);

    const hostConfig = config.compute(this.config.sshHost);

    // Portの型を適切に処理（数値または文字列の可能性）
    // ssh-configライブラリはプロパティ名の大文字・小文字が揺れる可能性があるため両方チェック
    let port = 22; // デフォルト
    if (this.config.port !== undefined) {
      port = this.config.port;
    } else if ((hostConfig as any).port !== undefined || hostConfig.Port !== undefined) {
      const portValue = (hostConfig as any).port ?? hostConfig.Port;
      port = Number(portValue);
    }

    const connectConfig: ConnectConfig = {
      host: String(hostConfig.HostName || ''),
      port,
      username: String(hostConfig.User || ''),
    };

    // 認証情報
    if (hostConfig.IdentityFile) {
      let identityFile = Array.isArray(hostConfig.IdentityFile)
        ? hostConfig.IdentityFile[0]
        : hostConfig.IdentityFile;
      // ~/ を展開
      if (typeof identityFile === 'string' && identityFile.startsWith('~/')) {
        identityFile = resolve(homedir(), identityFile.substring(2));
      }
      connectConfig.privateKey = readFileSync(String(identityFile));
    }

    return connectConfig;
  }

  /**
   * リモートバイナリパスを取得
   */
  private getRemoteBinaryPath(): string {
    return this.config.binaryPath || '~/.local/bin/kijuku-cli';
  }

  /**
   * リモートDBパスを取得
   */
  private getRemoteDbPath(): string {
    return this.config.dbPath || '~/.local/share/kijuku/kijuku.db';
  }

  /**
   * SSH接続を確立
   */
  private async connect(): Promise<void> {
    if (this.sshClient) {
      return; // 既に接続済み
    }

    return new Promise((resolve, reject) => {
      const client = new Client();
      const sshConfig = this.loadSSHConfig();

      client.on('ready', () => {
        this.sshClient = client;
        resolve();
      });

      client.on('error', (err) => {
        reject(new Error(`SSH接続エラー: ${err.message}`));
      });

      client.connect(sshConfig);
    });
  }

  /**
   * SSH接続を切断
   */
  private async disconnect(): Promise<void> {
    if (this.sshClient) {
      this.sshClient.end();
      this.sshClient = null;
    }
  }

  /**
   * リモートでコマンドを実行
   */
  private async execCommand(command: string): Promise<string> {
    if (!this.sshClient) {
      throw new Error('SSH接続が確立されていません');
    }

    return new Promise((resolve, reject) => {
      this.sshClient!.exec(command, (err, stream) => {
        if (err) {
          reject(new Error(`コマンド実行エラー: ${err.message}`));
          return;
        }

        let stdout = '';
        let stderr = '';

        stream.on('close', (code: number) => {
          if (code !== 0) {
            reject(new Error(`コマンドが失敗しました (exit code: ${code}): ${stderr}`));
          } else {
            resolve(stdout);
          }
        });

        stream.on('data', (data: Buffer) => {
          stdout += data.toString();
        });

        stream.stderr.on('data', (data: Buffer) => {
          stderr += data.toString();
        });
      });
    });
  }

  /**
   * リモートファイルの存在確認
   */
  private async checkFileExists(filePath: string): Promise<boolean> {
    try {
      await this.execCommand(`test -f ${filePath} && echo "exists"`);
      return true;
    } catch {
      return false;
    }
  }

  /**
   * ローカルからリモートにファイルを転送
   */
  private async uploadFile(localPath: string, remotePath: string): Promise<void> {
    if (!this.sshClient) {
      throw new Error('SSH接続が確立されていません');
    }

    return new Promise((resolve, reject) => {
      this.sshClient!.sftp((err, sftp) => {
        if (err) {
          reject(new Error(`SFTP接続エラー: ${err.message}`));
          return;
        }

        const localData = readFileSync(localPath);
        sftp.writeFile(remotePath, localData, (err) => {
          if (err) {
            reject(new Error(`ファイル転送エラー: ${err.message}`));
          } else {
            resolve();
          }
        });
      });
    });
  }

  /**
   * リモートにバイナリを自動デプロイ
   */
  private async deployBinaryToRemote(): Promise<void> {
    const localBinaryPath = resolve(homedir(), '.local', 'bin', 'kijuku-cli');
    const remoteBinaryPath = this.getRemoteBinaryPath();
    const remoteDir = remoteBinaryPath.substring(0, remoteBinaryPath.lastIndexOf('/'));

    // リモートにディレクトリ作成
    await this.execCommand(`mkdir -p ${remoteDir}`);

    // バイナリを転送
    await this.uploadFile(localBinaryPath, remoteBinaryPath);

    // 実行権限を付与
    await this.execCommand(`chmod +x ${remoteBinaryPath}`);
  }

  /**
   * リモートでJSONコマンドを実行
   */
  private async executeRemoteCommand(request: CommandRequest): Promise<CommandResponse> {
    await this.connect();

    try {
      // バイナリの存在確認
      const remoteBinaryPath = this.getRemoteBinaryPath();
      const exists = await this.checkFileExists(remoteBinaryPath);

      if (!exists) {
        // バイナリが存在しない場合は自動デプロイ
        await this.deployBinaryToRemote();
      }

      // コマンドを実行
      const remoteDbPath = this.getRemoteDbPath();
      const jsonInput = JSON.stringify(request);
      const command = `echo '${jsonInput}' | ${remoteBinaryPath} --db ${remoteDbPath}`;

      const output = await this.execCommand(command);
      const response: CommandResponse = JSON.parse(output.trim());

      return response;
    } finally {
      await this.disconnect();
    }
  }

  /**
   * レスポンスのエラーチェック
   */
  private checkResponse(response: CommandResponse): any {
    if (!response.success) {
      throw new Error(response.error || '不明なエラー');
    }
    return response.data;
  }

  /**
   * マイグレーションを実行
   */
  async migrate(): Promise<void> {
    const response = await this.executeRemoteCommand({
      operation: 'migrate',
      params: {},
    });
    this.checkResponse(response);
  }

  /**
   * 現在のスキーマバージョンを取得
   */
  async getSchemaVersion(): Promise<number> {
    const response = await this.executeRemoteCommand({
      operation: 'getSchemaVersion',
      params: {},
    });
    const data = this.checkResponse(response);
    return data.version;
  }

  /**
   * テーブル一覧を取得
   */
  async getTables(): Promise<string[]> {
    const response = await this.executeRemoteCommand({
      operation: 'getTables',
      params: {},
    });
    return this.checkResponse(response);
  }

  /**
   * 特定テーブルのカラム情報を取得
   */
  async getTableInfo(tableName: string): Promise<TableColumnInfo[]> {
    const response = await this.executeRemoteCommand({
      operation: 'getTableInfo',
      params: { table_name: tableName },
    });
    return this.checkResponse(response);
  }

  /**
   * メディアを作成
   */
  async createMedia(data: MediaInput): Promise<Media> {
    const response = await this.executeRemoteCommand({
      operation: 'createMedia',
      params: { data },
    });
    return this.checkResponse(response);
  }

  /**
   * IDでメディアを取得
   */
  async getMedia(id: number): Promise<Media | null> {
    const response = await this.executeRemoteCommand({
      operation: 'getMedia',
      params: { id },
    });
    return this.checkResponse(response);
  }

  /**
   * メディアを更新
   */
  async updateMedia(id: number, data: Partial<MediaInput>): Promise<void> {
    const response = await this.executeRemoteCommand({
      operation: 'updateMedia',
      params: { id, data },
    });
    this.checkResponse(response);
  }

  /**
   * メディアを削除
   */
  async deleteMedia(id: number): Promise<void> {
    const response = await this.executeRemoteCommand({
      operation: 'deleteMedia',
      params: { id },
    });
    this.checkResponse(response);
  }

  /**
   * メディアを検索
   */
  async findMedia(filter: MediaFilter, options?: QueryOptions): Promise<Media[]> {
    const response = await this.executeRemoteCommand({
      operation: 'findMedia',
      params: { filter, options },
    });
    return this.checkResponse(response);
  }

  /**
   * 複数のメディアを一括作成
   */
  async bulkCreateMedia(dataList: MediaInput[]): Promise<Media[]> {
    const response = await this.executeRemoteCommand({
      operation: 'bulkCreateMedia',
      params: { data_list: dataList },
    });
    return this.checkResponse(response);
  }

  /**
   * タグを作成
   */
  async createTag(name: string): Promise<Tag> {
    const response = await this.executeRemoteCommand({
      operation: 'createTag',
      params: { name },
    });
    return this.checkResponse(response);
  }

  /**
   * タグ名でタグを取得
   */
  async getTagByName(name: string): Promise<Tag | null> {
    const response = await this.executeRemoteCommand({
      operation: 'getTagByName',
      params: { name },
    });
    return this.checkResponse(response);
  }

  /**
   * 全てのタグを取得
   */
  async getAllTags(): Promise<Tag[]> {
    const response = await this.executeRemoteCommand({
      operation: 'getAllTags',
      params: {},
    });
    return this.checkResponse(response);
  }

  /**
   * メディアにタグを追加
   */
  async addTagToMedia(mediaId: number, tagId: number): Promise<void> {
    const response = await this.executeRemoteCommand({
      operation: 'addTagToMedia',
      params: { media_id: mediaId, tag_id: tagId },
    });
    this.checkResponse(response);
  }

  /**
   * メディアからタグを削除
   */
  async removeTagFromMedia(mediaId: number, tagId: number): Promise<void> {
    const response = await this.executeRemoteCommand({
      operation: 'removeTagFromMedia',
      params: { media_id: mediaId, tag_id: tagId },
    });
    this.checkResponse(response);
  }

  /**
   * メディアに関連付けられたタグを取得
   */
  async getMediaTags(mediaId: number): Promise<Tag[]> {
    const response = await this.executeRemoteCommand({
      operation: 'getMediaTags',
      params: { media_id: mediaId },
    });
    return this.checkResponse(response);
  }

  /**
   * メディアに属性を設定
   */
  async setMediaAttribute(
    mediaId: number,
    key: string,
    value: string | null,
    valueType?: string
  ): Promise<void> {
    const response = await this.executeRemoteCommand({
      operation: 'setMediaAttribute',
      params: { media_id: mediaId, key, value, value_type: valueType },
    });
    this.checkResponse(response);
  }

  /**
   * メディアの属性を取得
   */
  async getMediaAttribute(mediaId: number, key: string): Promise<MediaAttribute | null> {
    const response = await this.executeRemoteCommand({
      operation: 'getMediaAttribute',
      params: { media_id: mediaId, key },
    });
    return this.checkResponse(response);
  }

  /**
   * メディアの全ての属性を取得
   */
  async getMediaAttributes(mediaId: number): Promise<MediaAttribute[]> {
    const response = await this.executeRemoteCommand({
      operation: 'getMediaAttributes',
      params: { media_id: mediaId },
    });
    return this.checkResponse(response);
  }

  /**
   * メディアの属性を削除
   */
  async deleteMediaAttribute(mediaId: number, key: string): Promise<void> {
    const response = await this.executeRemoteCommand({
      operation: 'deleteMediaAttribute',
      params: { media_id: mediaId, key },
    });
    this.checkResponse(response);
  }

  /**
   * メディアの全ての属性を削除
   */
  async deleteAllMediaAttributes(mediaId: number): Promise<void> {
    const response = await this.executeRemoteCommand({
      operation: 'deleteAllMediaAttributes',
      params: { media_id: mediaId },
    });
    this.checkResponse(response);
  }
}
