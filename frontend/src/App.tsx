import { useEffect } from "react";
import { Navigate, Route, Routes, useLocation, useNavigate } from "react-router-dom";
import { Layout } from "./components/Layout";
import { ToastHost } from "./components/ui";
import { useApi } from "./lib/hooks";
import type { Config } from "./lib/types";
import Overview from "./pages/Overview";
import Library from "./pages/Library";
import Compression from "./pages/Compression";
import Comparison from "./pages/Comparison";
import Inference from "./pages/Inference";
import Monitor from "./pages/Monitor";
import History from "./pages/History";
import SettingsPage from "./pages/Settings";
import Onboarding from "./pages/Onboarding";
import RuntimeOptimization from "./pages/RuntimeOptimization";
import BenchmarkCenter from "./pages/BenchmarkCenter";
import MicroProjects from "./pages/MicroProjects";
import MicroStudio from "./pages/MicroStudio";
import TrainingWorkspace from "./pages/TrainingWorkspace";
import DistillationLab from "./pages/DistillationLab";
import HardwareLab from "./pages/HardwareLab";
import DeploymentCenter from "./pages/DeploymentCenter";

function FirstRunGate() {
  const { data: config } = useApi<Config>("/settings");
  const location = useLocation();
  const navigate = useNavigate();

  useEffect(() => {
    if (!config) return;
    if (config.onboarding_completed) {
      window.localStorage.setItem("atom.onboarded", "1");
      return;
    }
    if (window.localStorage.getItem("atom.onboarded") === "1") return;
    if (location.pathname !== "/welcome") navigate("/welcome", { replace: true });
  }, [config, location.pathname, navigate]);

  return null;
}

export default function App() {
  return (
    <>
      <FirstRunGate />
      <Layout>
        <Routes>
          <Route path="/" element={<Overview />} />
          <Route path="/welcome" element={<Onboarding />} />
          <Route path="/library" element={<Library />} />
          <Route path="/compression" element={<Compression />} />
          <Route path="/runtime" element={<RuntimeOptimization />} />
          <Route path="/comparison" element={<Comparison />} />
          <Route path="/benchmarks" element={<BenchmarkCenter />} />
          <Route path="/inference" element={<Inference />} />
          <Route path="/monitor" element={<Monitor />} />
          <Route path="/micro" element={<MicroProjects />} />
          <Route path="/micro/:id" element={<MicroStudio />} />
          <Route path="/training" element={<TrainingWorkspace />} />
          <Route path="/distillation" element={<DistillationLab />} />
          <Route path="/hardware" element={<HardwareLab />} />
          <Route path="/deployment" element={<DeploymentCenter />} />
          <Route path="/history" element={<History />} />
          <Route path="/settings" element={<SettingsPage />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </Layout>
      <ToastHost />
    </>
  );
}
