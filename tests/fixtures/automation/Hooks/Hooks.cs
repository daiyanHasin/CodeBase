[Binding] public class Hooks { [BeforeScenario] public void Before() { DriverFactory.Start(); } [AfterScenario] public void After() { DriverFactory.Stop(); } }
