import { desktopApi } from './api/desktop';
import { ProcessingView } from './features/processing/ProcessingView';
export default function App() {return <ProcessingView api={desktopApi} />;}
