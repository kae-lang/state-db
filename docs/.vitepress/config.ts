import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'SMQL Engine',
  description: 'State Machine Query Language — a database engine that understands lifecycles',

  themeConfig: {
    search: {
      provider: 'local'
    },

    outline: {
      level: [2, 3]
    },

    nav: [
      {
        text: 'Learn',
        items: [
          { text: 'What is SMQL?', link: '/introduction/what-is-smql' },
          { text: 'Quick Start', link: '/introduction/quick-start' },
          { text: 'Tutorials', link: '/tutorials/' },
        ]
      },
      { text: 'Language', link: '/language/define-machine' },
      {
        text: 'Commands',
        items: [
          { text: 'SPAWN', link: '/commands/spawn' },
          { text: 'TRANSITION', link: '/commands/transition' },
          { text: 'ALTER MACHINE', link: '/commands/alter-machine' },
        ]
      },
      { text: 'Queries', link: '/queries/get' },
      {
        text: 'Deploy',
        items: [
          { text: 'HTTP Server', link: '/server/getting-started' },
          { text: 'Rust SDK', link: '/sdk/getting-started' },
          { text: 'CLI', link: '/cli/overview' },
        ]
      },
      {
        text: 'Reference',
        items: [
          { text: 'Expressions', link: '/reference/expressions' },
          { text: 'Built-in Functions', link: '/reference/built-in-functions' },
          { text: 'Configuration', link: '/reference/configuration' },
          { text: 'Error Types', link: '/reference/error-types' },
          { text: 'Grammar', link: '/reference/grammar' },
          { text: 'Reserved Words', link: '/reference/reserved-words' },
        ]
      },
    ],

    sidebar: {
      '/introduction/': [
        {
          text: 'Introduction',
          collapsed: false,
          items: [
            { text: 'What is SMQL?', link: '/introduction/what-is-smql' },
            { text: 'Why SMQL?', link: '/introduction/why-smql' },
            { text: 'Quick Start', link: '/introduction/quick-start' },
            { text: 'Key Concepts', link: '/introduction/key-concepts' },
          ]
        }
      ],

      '/tutorials/': [
        {
          text: 'Tutorials',
          collapsed: false,
          items: [
            { text: 'Overview', link: '/tutorials/' },
            { text: '1. Your First Machine', link: '/tutorials/your-first-machine' },
            { text: '2. Data & Guards', link: '/tutorials/adding-data-and-guards' },
            { text: '3. Timeouts & Hooks', link: '/tutorials/timeouts-and-hooks' },
            { text: '4. Composition', link: '/tutorials/composition-patterns' },
            { text: '5. Queries & Analytics', link: '/tutorials/queries-and-analytics' },
            { text: '6. Production', link: '/tutorials/production-deployment' },
          ]
        }
      ],

      '/language/': [
        {
          text: 'Language Reference',
          collapsed: false,
          items: [
            { text: 'DEFINE MACHINE', link: '/language/define-machine' },
            { text: 'Data Types', link: '/language/data-types' },
            { text: 'Constraints', link: '/language/constraints' },
            { text: 'States & Transitions', link: '/language/states-and-transitions' },
            { text: 'Guards', link: '/language/guards' },
            { text: 'Actions', link: '/language/actions' },
            { text: 'Mutations', link: '/language/mutations' },
            { text: 'Wildcards', link: '/language/wildcards' },
            { text: 'Timeouts', link: '/language/timeouts' },
            { text: 'Hooks', link: '/language/hooks' },
          ]
        }
      ],

      '/commands/': [
        {
          text: 'Commands',
          collapsed: false,
          items: [
            { text: 'SPAWN', link: '/commands/spawn' },
            { text: 'TRANSITION', link: '/commands/transition' },
            { text: 'TRY TRANSITION', link: '/commands/try-transition' },
            { text: 'TRANSITION ALL', link: '/commands/transition-all' },
            { text: 'ALTER MACHINE', link: '/commands/alter-machine' },
          ]
        }
      ],

      '/queries/': [
        {
          text: 'Queries',
          collapsed: false,
          items: [
            { text: 'GET', link: '/queries/get' },
            { text: 'FIND', link: '/queries/find' },
            { text: 'Aggregate', link: '/queries/aggregate' },
            { text: 'TRAIL', link: '/queries/trail' },
            { text: 'PATHS', link: '/queries/paths' },
            { text: 'FUNNEL', link: '/queries/funnel' },
            { text: 'COMPARE PATHS', link: '/queries/compare-paths' },
            { text: 'Filter Predicates', link: '/queries/filter-predicates' },
          ]
        }
      ],

      '/composition/': [
        {
          text: 'Composition',
          collapsed: false,
          items: [
            { text: 'Overview', link: '/composition/overview' },
            { text: 'CHILDREN Declaration', link: '/composition/children-declaration' },
            { text: 'Spawning Children', link: '/composition/spawning-children' },
            { text: 'Child Predicates', link: '/composition/child-predicates' },
            { text: 'SIGNAL PARENT', link: '/composition/signal-parent' },
            { text: 'CASCADE', link: '/composition/cascade' },
          ]
        }
      ],

      '/server/': [
        {
          text: 'HTTP Server',
          collapsed: false,
          items: [
            { text: 'Getting Started', link: '/server/getting-started' },
            { text: 'HTTP API', link: '/server/http-api' },
            { text: 'Request & Response', link: '/server/request-response' },
            { text: 'WebSocket', link: '/server/websocket' },
            { text: 'Storage Backends', link: '/server/storage-backends' },
            { text: 'Observability', link: '/server/observability' },
          ]
        }
      ],

      '/cli/': [
        {
          text: 'CLI',
          collapsed: false,
          items: [
            { text: 'Overview', link: '/cli/overview' },
            { text: 'serve', link: '/cli/serve' },
            { text: 'repl', link: '/cli/repl' },
            { text: 'exec', link: '/cli/exec' },
            { text: 'run', link: '/cli/run' },
            { text: 'codegen', link: '/cli/codegen' },
          ]
        }
      ],

      '/sdk/': [
        {
          text: 'Rust SDK',
          collapsed: false,
          items: [
            { text: 'Getting Started', link: '/sdk/getting-started' },
            { text: 'Client API', link: '/sdk/client-api' },
            { text: 'Queries', link: '/sdk/queries' },
            { text: 'WebSocket Subscriptions', link: '/sdk/websocket-subscriptions' },
            { text: 'Typed API', link: '/sdk/typed-api' },
            { text: 'Code Generation', link: '/sdk/codegen' },
            { text: 'Error Handling', link: '/sdk/error-handling' },
          ]
        }
      ],

      '/guides/': [
        {
          text: 'Guides',
          collapsed: false,
          items: [
            { text: 'Support Ticket', link: '/guides/support-ticket' },
            { text: 'Order Processing', link: '/guides/order-processing' },
            { text: 'CI/CD Pipeline', link: '/guides/ci-pipeline' },
            { text: 'Schema Evolution', link: '/guides/schema-evolution' },
          ]
        }
      ],

      '/architecture/': [
        {
          text: 'Architecture',
          collapsed: false,
          items: [
            { text: 'Overview', link: '/architecture/overview' },
            { text: 'Execution Flow', link: '/architecture/execution-flow' },
            { text: 'Storage Layer', link: '/architecture/storage-layer' },
            { text: 'Timer System', link: '/architecture/timer-system' },
            { text: 'Expression Evaluator', link: '/architecture/expression-evaluator' },
            { text: 'Concurrency Model', link: '/architecture/concurrency-model' },
          ]
        }
      ],

      '/reference/': [
        {
          text: 'Reference',
          collapsed: false,
          items: [
            { text: 'Expressions', link: '/reference/expressions' },
            { text: 'Built-in Functions', link: '/reference/built-in-functions' },
            { text: 'Configuration', link: '/reference/configuration' },
            { text: 'Error Types', link: '/reference/error-types' },
            { text: 'Reserved Words', link: '/reference/reserved-words' },
            { text: 'Grammar', link: '/reference/grammar' },
          ]
        }
      ],
    },

    socialLinks: [
      { icon: 'github', link: 'https://github.com/anthropics/smql-engine' }
    ],

    editLink: {
      pattern: 'https://github.com/anthropics/smql-engine/edit/main/docs/:path'
    },

    footer: {
      message: 'Released under the MIT License.',
    },
  }
})
