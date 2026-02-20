<template>
    <transition name="slide-fade" appear>
        <div>
            <h1 class="mb-3">
                <router-link :to="composeRoute">{{ stackName }}</router-link> - {{ serviceName }} ({{ $t("Logs") }})
            </h1>

            <Terminal
                class="terminal"
                :rows="20"
                mode="containerLogs"
                :name="terminalName"
                :stack-name="stackName"
                :service-name="serviceName"
                :endpoint="endpoint"
            ></Terminal>
        </div>
    </transition>
</template>

<script>
import { getContainerLogsTerminalName } from "../../common/util-common";

export default {
    components: {},
    data() {
        return {};
    },
    computed: {
        stackName() {
            return this.$route.params.stackName;
        },
        endpoint() {
            return this.$route.params.endpoint || "";
        },
        serviceName() {
            return this.$route.params.serviceName;
        },
        composeRoute() {
            if (this.endpoint) {
                return `/compose/${this.stackName}/${this.endpoint}`;
            }
            return `/compose/${this.stackName}`;
        },
        terminalName() {
            return getContainerLogsTerminalName(
                this.endpoint,
                this.stackName,
                this.serviceName,
            );
        },
    },
    mounted() {},
    methods: {},
};
</script>

<style scoped lang="scss">
.terminal {
    height: 410px;
}
</style>
