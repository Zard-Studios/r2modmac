import https from 'https';
import fs from 'fs';
import path from 'path';
import AdmZip from 'adm-zip';

export class ModManager {
    static async downloadMod(url: string, destinationPath: string): Promise<string> {
        return new Promise((resolve, reject) => {
            const file = fs.createWriteStream(destinationPath);

            https.get(url, (response) => {
                if (response.statusCode === 302 || response.statusCode === 301) {
                    // Follow redirect
                    const redirectUrl = response.headers.location;
                    if (!redirectUrl) {
                        reject(new Error('Redirect without location'));
                        return;
                    }
                    https.get(redirectUrl, (redirectResponse) => {
                        redirectResponse.pipe(file);
                        file.on('finish', () => {
                            file.close();
                            resolve(destinationPath);
                        });
                    }).on('error', reject);
                } else {
                    response.pipe(file);
                    file.on('finish', () => {
                        file.close();
                        resolve(destinationPath);
                    });
                }
            }).on('error', (err) => {
                fs.unlink(destinationPath, () => reject(err));
            });
        });
    }

    static extractZip(zipPath: string, extractTo: string): void {
        const zip = new AdmZip(zipPath);
        zip.extractAllTo(extractTo, true);
    }

    static async installMod(
        downloadUrl: string,
        modName: string,
        gameDir: string,
        tempDir: string
    ): Promise<void> {
        // Create temp directory if it doesn't exist
        if (!fs.existsSync(tempDir)) {
            fs.mkdirSync(tempDir, { recursive: true });
        }

        // Download the mod
        const zipPath = path.join(tempDir, `${modName}.zip`);
        await this.downloadMod(downloadUrl, zipPath);

        // Extract to game directory
        const modInstallPath = path.join(gameDir, 'BepInEx', 'plugins', modName);
        if (!fs.existsSync(modInstallPath)) {
            fs.mkdirSync(modInstallPath, { recursive: true });
        }

        this.extractZip(zipPath, modInstallPath);

        // Clean up
        fs.unlinkSync(zipPath);
    }
}
