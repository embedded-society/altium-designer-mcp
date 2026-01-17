/**
 * GitHub Actions Cache Cleanup Script
 *
 * Keeps only the most recent cache per group (rust-Windows, rust-Linux, etc.)
 * and deletes all older/stale caches.
 */

module.exports = async ({ github, context, core }) => {
    const dryRun = process.env.INPUT_DRY_RUN === "true";
    const owner = context.repo.owner;
    const repo = context.repo.repo;

    const stats = {
        totalCaches: 0,
        deletedCaches: 0,
        freedBytes: 0,
        errors: 0,
    };

    function formatBytes(bytes) {
        if (bytes === 0) return "0 B";
        const k = 1024;
        const sizes = ["B", "KB", "MB", "GB"];
        const i = Math.floor(Math.log(bytes) / Math.log(k));
        return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
    }

    function getDaysSinceAccess(cache) {
        const lastAccessed = new Date(cache.last_accessed_at || cache.created_at);
        const now = new Date();
        return Math.floor((now - lastAccessed) / (1000 * 60 * 60 * 24));
    }

    function log(message, level = "info") {
        const prefix = dryRun ? "[DRY-RUN] " : "";
        if (level === "error") {
            core.error(`${prefix}${message}`);
        } else if (level === "warning") {
            core.warning(`${prefix}${message}`);
        } else {
            core.info(`${prefix}${message}`);
        }
    }

    async function listAllCaches() {
        const caches = [];
        let page = 1;

        while (true) {
            try {
                const response = await github.rest.actions.getActionsCacheList({
                    owner,
                    repo,
                    per_page: 100,
                    page,
                });

                if (response.data.actions_caches.length === 0) break;
                caches.push(...response.data.actions_caches);
                if (response.data.actions_caches.length < 100) break;
                page++;
            } catch (error) {
                if (error.status === 404) {
                    log("No caches found", "warning");
                    break;
                }
                throw error;
            }
        }

        return caches;
    }

    async function deleteCache(cache, reason) {
        if (dryRun) {
            log(`  Would delete: ${cache.key} (${formatBytes(cache.size_in_bytes)}) [${reason}]`);
            stats.freedBytes += cache.size_in_bytes;
            return true;
        }

        try {
            await github.rest.actions.deleteActionsCacheById({
                owner,
                repo,
                cache_id: cache.id,
            });
            log(`  Deleted: ${cache.key} (${formatBytes(cache.size_in_bytes)}) [${reason}]`);
            stats.freedBytes += cache.size_in_bytes;
            return true;
        } catch (error) {
            log(`  Failed to delete ${cache.key}: ${error.message}`, "error");
            stats.errors++;
            return false;
        }
    }

    try {
        log("=".repeat(60));
        log("GitHub Actions Cache Cleanup");
        log("=".repeat(60));
        log("");
        log(`Repository:   ${owner}/${repo}`);
        log(`Dry run:      ${dryRun}`);
        log("");

        const caches = await listAllCaches();
        stats.totalCaches = caches.length;

        if (caches.length === 0) {
            log("No caches found.");
            return;
        }

        log(`Found ${caches.length} cache(s).`);
        log("");

        // Group caches by type and OS
        // Rust caches: rust-{OS}-{rustc-hash}-{cargo-lock-hash}
        // npm caches: npm-markdownlint-{OS}-node-{version}-mdlint-{version}
        const cacheGroups = {};
        for (const cache of caches) {
            let prefix;
            if (cache.key.startsWith("rust-")) {
                // Group by rust-{OS} (e.g., rust-Linux, rust-Windows, rust-macOS)
                const parts = cache.key.split("-");
                prefix = `${parts[0]}-${parts[1]}`;
            } else if (cache.key.startsWith("npm-markdownlint-")) {
                // Group by npm-markdownlint-{OS} (e.g., npm-markdownlint-Linux)
                const parts = cache.key.split("-");
                prefix = `${parts[0]}-${parts[1]}-${parts[2]}`;
            } else {
                // Unknown cache type, use full key as prefix (won't group)
                prefix = cache.key;
            }

            if (!cacheGroups[prefix]) {
                cacheGroups[prefix] = [];
            }
            cacheGroups[prefix].push(cache);
        }

        for (const [prefix, groupCaches] of Object.entries(cacheGroups)) {
            // Sort by last_accessed_at descending (most recent first)
            groupCaches.sort((a, b) => new Date(b.last_accessed_at) - new Date(a.last_accessed_at));

            log(`Group: ${prefix} (${groupCaches.length} cache(s))`);

            // Keep the first (most recent), delete the rest
            if (groupCaches.length > 0) {
                log(`  Keeping: ${groupCaches[0].key}`);
            }

            for (let i = 1; i < groupCaches.length; i++) {
                const cache = groupCaches[i];
                const daysSinceAccess = getDaysSinceAccess(cache);
                if (await deleteCache(cache, `stale, ${daysSinceAccess}d since last access`)) {
                    stats.deletedCaches++;
                }
            }
        }

        log("");
        log("=".repeat(60));
        log("Summary");
        log("=".repeat(60));
        log(`  Total caches:  ${stats.totalCaches}`);
        log(`  Deleted:       ${stats.deletedCaches}`);
        log(`  Kept:          ${stats.totalCaches - stats.deletedCaches}`);
        log(`  Space freed:   ${formatBytes(stats.freedBytes)}`);

        if (stats.errors > 0) {
            log(`  Errors:        ${stats.errors}`, "warning");
        }

        if (dryRun) {
            log("");
            log("Dry run - no caches were actually deleted.", "warning");
        }

        core.setOutput("deleted_count", stats.deletedCaches);
        core.setOutput("freed_bytes", stats.freedBytes);

        log("");
        log("Cleanup completed");
    } catch (error) {
        core.setFailed(`Cleanup failed: ${error.message}`);
        throw error;
    }
};
