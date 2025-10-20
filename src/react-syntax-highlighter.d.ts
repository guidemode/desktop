// Type declarations for react-syntax-highlighter ESM imports
declare module 'react-syntax-highlighter/dist/esm/prism.js' {
  import type { ComponentType } from 'react'

  interface SyntaxHighlighterProps {
    language?: string
    style?: any
    customStyle?: any
    showLineNumbers?: boolean
    children?: string
    [key: string]: any
  }

  const Prism: ComponentType<SyntaxHighlighterProps>
  export default Prism
}

declare module 'react-syntax-highlighter/dist/esm/styles/prism/one-dark.js' {
  const style: { [key: string]: React.CSSProperties }
  export default style
}

declare module 'react-syntax-highlighter/dist/esm/styles/prism/one-light.js' {
  const style: { [key: string]: React.CSSProperties }
  export default style
}
