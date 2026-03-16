<template>
  <transition name="slide-fade" appear>
    <div>
      <h1 class="mb-3">
        <router-link :to="composeRoute">{{ stackName }}</router-link>
        {{ ' - ' }}
        {{ serviceName }}
      </h1>

      <div class="mb-4 action-buttons">
        <button
          class="btn btn-normal me-2"
          :disabled="processing"
          @click="forcePull"
        >
          <font-awesome-icon icon="download" />
          {{ $t('Force Pull') }}
        </button>
        <button
          class="btn btn-normal me-2"
          :disabled="processing"
          @click="restart"
        >
          <font-awesome-icon icon="rotate" />
          {{ $t('Restart') }}
        </button>
        <button
          v-if="isRunning"
          class="btn btn-danger me-2"
          :disabled="processing"
          @click="stop"
        >
          <font-awesome-icon icon="stop" />
          {{ $t('Stop') }}
        </button>
        <button
          v-else
          class="btn btn-primary me-2"
          :disabled="processing"
          @click="start"
        >
          <font-awesome-icon icon="play" />
          {{ $t('Start') }}
        </button>
        <router-link class="btn btn-normal me-2" :to="logsRoute">
          <font-awesome-icon icon="file-lines" />
          {{ $t('Logs') }}
        </router-link>
        <router-link class="btn btn-normal" :to="terminalRoute">
          <font-awesome-icon icon="terminal" />
          Bash
        </router-link>
      </div>

      <div v-if="msg" class="alert" :class="msgClass" role="alert">
        {{ msg }}
      </div>

      <div class="shadow-box big-padding status-box mb-3">
        <div class="mb-2">
          <span class="badge me-1" :class="bgStyle">{{ status }}</span>
          <span v-if="health" class="badge me-1" :class="healthStyle">{{
            health
          }}</span>
          <a
            v-for="port in ports"
            :key="port"
            :href="parsePort(port).url"
            target="_blank"
          >
            <span class="badge me-1 bg-secondary">{{
              parsePort(port).display
            }}</span>
          </a>
        </div>
        <div v-if="image" class="image">
          <span class="me-1">{{ imageName }}:</span
          ><span class="tag">{{ imageTag }}</span>
        </div>
      </div>

      <Terminal
        class="terminal"
        :rows="20"
        mode="containerLogs"
        :name="terminalName"
        :stack-name="stackName"
        :service-name="serviceName"
        :endpoint="endpoint"
      />
    </div>
  </transition>
</template>

<script>
import { FontAwesomeIcon } from '@fortawesome/vue-fontawesome'
import {
  parseDockerPort,
  getContainerLogsTerminalName,
} from '../../common/util-common'
import Terminal from '../components/Terminal.vue'

export default {
  components: { FontAwesomeIcon, Terminal },
  data() {
    return {
      status: 'N/A',
      health: null,
      ports: [],
      image: null,
      processing: false,
      msg: null,
      msgOk: true,
      statusTimeout: null,
    }
  },
  computed: {
    stackName() {
      return this.$route.params.stackName
    },
    serviceName() {
      return this.$route.params.serviceName
    },
    endpoint() {
      return this.$route.params.endpoint || ''
    },
    isRunning() {
      return this.status === 'running'
    },
    bgStyle() {
      if (this.status === 'running') return 'bg-primary'
      if (this.status === 'exited' || this.status === 'dead') return 'bg-danger'
      return 'bg-secondary'
    },
    healthStyle() {
      if (this.health === 'healthy') return 'bg-success'
      if (this.health === 'unhealthy') return 'bg-danger'
      return 'bg-secondary'
    },
    imageName() {
      if (!this.image) return ''
      return this.image.split(':')[0]
    },
    imageTag() {
      if (!this.image) return ''
      const tag = this.image.split(':')[1]
      return tag || 'latest'
    },
    msgClass() {
      return this.msgOk ? 'alert-success' : 'alert-danger'
    },
    composeRoute() {
      if (this.endpoint) return `/compose/${this.stackName}/${this.endpoint}`
      return `/compose/${this.stackName}`
    },
    logsRoute() {
      if (this.endpoint) {
        return {
          name: 'containerLogsEndpoint',
          params: {
            stackName: this.stackName,
            serviceName: this.serviceName,
            endpoint: this.endpoint,
          },
        }
      }
      return {
        name: 'containerLogs',
        params: { stackName: this.stackName, serviceName: this.serviceName },
      }
    },
    terminalName() {
      return getContainerLogsTerminalName(
        this.endpoint,
        this.stackName,
        this.serviceName,
      )
    },
    terminalRoute() {
      if (this.endpoint) {
        return {
          name: 'containerTerminalEndpoint',
          params: {
            stackName: this.stackName,
            serviceName: this.serviceName,
            type: 'bash',
            endpoint: this.endpoint,
          },
        }
      }
      return {
        name: 'containerTerminal',
        params: {
          stackName: this.stackName,
          serviceName: this.serviceName,
          type: 'bash',
        },
      }
    },
  },
  mounted() {
    this.loadStatus()
  },
  unmounted() {
    if (this.statusTimeout) clearTimeout(this.statusTimeout)
  },
  methods: {
    loadStatus() {
      this.$root.emitAgent(
        this.endpoint,
        'serviceStatusList',
        this.stackName,
        (res) => {
          if (res.ok && res.serviceStatusList) {
            const svc = res.serviceStatusList[this.serviceName]
            if (svc) {
              this.status = svc.state || 'N/A'
              this.health = svc.health || null
              this.ports = svc.ports || []
              this.image = svc.image || null
            }
          }
        },
      )
    },
    parsePort(port) {
      const hostname = this.$root.info?.primaryHostname || location.hostname
      return parseDockerPort(port, hostname)
    },
    showMsg(ok, text) {
      this.msgOk = ok
      this.msg = text
      setTimeout(() => {
        this.msg = null
      }, 3000)
    },
    restart() {
      this.processing = true
      this.$root.emitAgent(
        this.endpoint,
        'restartService',
        this.stackName,
        this.serviceName,
        (res) => {
          this.processing = false
          if (res.ok) {
            this.showMsg(true, 'Restarted')
            this.loadStatus()
          } else {
            this.showMsg(false, res.msg || 'Failed')
          }
        },
      )
    },
    start() {
      this.processing = true
      this.$root.emitAgent(
        this.endpoint,
        'startService',
        this.stackName,
        this.serviceName,
        (res) => {
          this.processing = false
          if (res.ok) {
            this.showMsg(true, 'Started')
            this.loadStatus()
          } else {
            this.showMsg(false, res.msg || 'Failed')
          }
        },
      )
    },
    stop() {
      this.processing = true
      this.$root.emitAgent(
        this.endpoint,
        'stopService',
        this.stackName,
        this.serviceName,
        (res) => {
          this.processing = false
          if (res.ok) {
            this.showMsg(true, 'Stopped')
            this.loadStatus()
          } else {
            this.showMsg(false, res.msg || 'Failed')
          }
        },
      )
    },
    forcePull() {
      this.processing = true
      this.$root.emitAgent(
        this.endpoint,
        'pullService',
        this.stackName,
        this.serviceName,
        (res) => {
          this.processing = false
          if (res.ok) {
            this.showMsg(true, 'Image pulled')
            this.loadStatus()
          } else {
            this.showMsg(false, res.msg || 'Failed')
          }
        },
      )
    },
  },
}
</script>

<style scoped lang="scss">
.terminal {
  height: 410px;
}

.action-buttons {
  display: flex;
  flex-wrap: wrap;
  gap: 0;
}

.status-box {
  .image {
    font-size: 0.8rem;
    color: #6c757d;
    .tag {
      color: #33383b;
    }
  }
}
</style>
