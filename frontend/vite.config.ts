import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import Components from "unplugin-vue-components/vite";
import { BootstrapVueNextResolver } from "unplugin-vue-components/resolvers";
import viteCompression from "vite-plugin-compression";
import "vue";

const viteCompressionFilter = /\.(js|mjs|json|css|html|svg)$/i;

const backendPort = process.env.VITE_BACKEND_PORT ?? "5001";

// https://vitejs.dev/config/
export default defineConfig({
    server: {
        port: 5001,
        proxy: {
            "/socket.io": {
                target: `http://localhost:${backendPort}`,
                ws: true,
            },
        },
    },
    define: {
        FRONTEND_VERSION: JSON.stringify(process.env.npm_package_version),
    },
    build: {
        outDir: "../frontend-dist",
    },
    plugins: [
        vue(),
        Components({
            resolvers: [BootstrapVueNextResolver()],
        }),
        viteCompression({
            algorithm: "gzip",
            filter: viteCompressionFilter,
        }),
        viteCompression({
            algorithm: "brotliCompress",
            filter: viteCompressionFilter,
        }),
    ],
});
