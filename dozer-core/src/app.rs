use dozer_types::models::flags::{EnableProbabilisticOptimizations, Flags};
use dozer_types::node::NodeHandle;

use crate::appsource::{self, AppSourceManager};
use crate::errors::ExecutionError;
use crate::node::{PortHandle, ProcessorFactory, SinkFactory};
use crate::{Dag, Edge, Endpoint};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PipelineEntryPoint {
    source_name: String,
    /// Target port.
    port: PortHandle,
}

impl PipelineEntryPoint {
    pub fn new(source_name: String, port: PortHandle) -> Self {
        Self { source_name, port }
    }

    pub fn source_name(&self) -> &str {
        &self.source_name
    }
}

#[derive(Debug)]
pub struct AppPipeline {
    edges: Vec<Edge>,
    processors: Vec<(NodeHandle, Box<dyn ProcessorFactory>)>,
    sinks: Vec<(NodeHandle, Box<dyn SinkFactory>)>,
    entry_points: Vec<(NodeHandle, PipelineEntryPoint)>,
    flags: PipelineFlags,
}

impl AppPipeline {
    pub fn add_processor(
        &mut self,
        proc: Box<dyn ProcessorFactory>,
        id: &str,
        entry_point: Vec<PipelineEntryPoint>,
    ) {
        let handle = NodeHandle::new(None, id.to_string());
        self.processors.push((handle.clone(), proc));

        for p in entry_point {
            self.entry_points.push((handle.clone(), p));
        }
    }

    pub fn add_sink(
        &mut self,
        sink: Box<dyn SinkFactory>,
        id: &str,
        entry_point: Option<PipelineEntryPoint>,
    ) {
        let handle = NodeHandle::new(None, id.to_string());
        self.sinks.push((handle.clone(), sink));

        if let Some(entry_point) = entry_point {
            self.entry_points.push((handle, entry_point));
        }
    }

    pub fn connect_nodes(
        &mut self,
        from: &str,
        from_port: PortHandle,
        to: &str,
        to_port: PortHandle,
    ) {
        let edge = Edge::new(
            Endpoint::new(NodeHandle::new(None, from.to_string()), from_port),
            Endpoint::new(NodeHandle::new(None, to.to_string()), to_port),
        );
        self.edges.push(edge);
    }

    pub fn new(flags: PipelineFlags) -> Self {
        Self {
            processors: Vec::new(),
            sinks: Vec::new(),
            edges: Vec::new(),
            entry_points: Vec::new(),
            flags,
        }
    }

    pub fn new_with_default_flags() -> Self {
        Self::new(Default::default())
    }

    pub fn get_entry_points_sources_names(&self) -> Vec<String> {
        self.entry_points
            .iter()
            .map(|(_, p)| p.source_name().to_string())
            .collect()
    }

    pub fn flags(&self) -> &PipelineFlags {
        &self.flags
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineFlags {
    pub enable_probabilistic_optimizations: EnableProbabilisticOptimizations,
}

impl From<&Flags> for PipelineFlags {
    fn from(flags: &Flags) -> Self {
        Self {
            enable_probabilistic_optimizations: flags.enable_probabilistic_optimizations.clone(),
        }
    }
}

impl From<Flags> for PipelineFlags {
    fn from(flags: Flags) -> Self {
        Self::from(&flags)
    }
}

impl Default for PipelineFlags {
    fn default() -> Self {
        Flags::default().into()
    }
}

pub struct App {
    pipelines: Vec<(u16, AppPipeline)>,
    app_counter: u16,
    sources: AppSourceManager,
}

impl App {
    pub fn add_pipeline(&mut self, pipeline: AppPipeline) {
        self.app_counter += 1;
        self.pipelines.push((self.app_counter, pipeline));
    }

    pub fn into_dag(self) -> Result<Dag, ExecutionError> {
        let mut dag = Dag::new();
        // (source name, target endpoint)
        let mut entry_points: Vec<(String, Endpoint)> = Vec::new();

        // Add all processors and sinks while collecting the entry points.
        for (pipeline_id, pipeline) in self.pipelines {
            for (handle, proc) in pipeline.processors {
                dag.add_processor(NodeHandle::new(Some(pipeline_id), handle.id), proc);
            }
            for (handle, sink) in pipeline.sinks {
                dag.add_sink(NodeHandle::new(Some(pipeline_id), handle.id), sink);
            }
            for edge in pipeline.edges {
                dag.connect(
                    Endpoint::new(
                        NodeHandle::new(Some(pipeline_id), edge.from.node.id),
                        edge.from.port,
                    ),
                    Endpoint::new(
                        NodeHandle::new(Some(pipeline_id), edge.to.node.id),
                        edge.to.port,
                    ),
                )?;
            }

            for (handle, entry) in pipeline.entry_points {
                entry_points.push((
                    entry.source_name,
                    Endpoint::new(NodeHandle::new(Some(pipeline_id), handle.id), entry.port),
                ));
            }
        }

        for (source, mapping) in self.sources.sources.into_iter().zip(&self.sources.mappings) {
            let node_handle = NodeHandle::new(None, mapping.connection.clone());
            dag.add_source(node_handle, source);
        }

        // Connect to all pipelines
        for (source_name, target_endpoint) in entry_points {
            let source_endpoint =
                appsource::get_endpoint_from_mappings(&self.sources.mappings, &source_name)?;
            dag.connect(source_endpoint, target_endpoint)?;
        }

        Ok(dag)
    }

    pub fn new(sources: AppSourceManager) -> Self {
        Self {
            pipelines: Vec::new(),
            app_counter: 0,
            sources,
        }
    }
}
