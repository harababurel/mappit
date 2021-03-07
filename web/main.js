    fetch('subreddit_graph.json').then(res => res.json()).then(data => {
        console.log(data);
        const elem = document.getElementById('graph');

        const Graph = ForceGraph()(elem)
            .graphData(data)
            .nodeId('id')
            .nodeAutoColorBy('group')
            .nodeCanvasObject((node, ctx, globalScale) => {
                const label = node.id;

                const scale = typeof node.scale === 'undefined' ? 1 : node.scale;

                const fontSize = 12 * scale / globalScale;
                ctx.font = `${fontSize}px sans-serif`;
                const textWidth = ctx.measureText(label).width;
                const bckgDimensions = [textWidth, fontSize].map(n => n + fontSize * 0.2); // some padding

                ctx.fillStyle = 'rgba(255, 255, 255, 0.8)';
                ctx.fillRect(node.x - bckgDimensions[0] / 2, node.y - bckgDimensions[1] / 2, ...bckgDimensions);

                ctx.textAlign = 'center';
                ctx.textBaseline = 'middle';
                ctx.fillStyle = node.color;
                ctx.fillText(label, node.x, node.y);

                node.__bckgDimensions = bckgDimensions; // to re-use in nodePointerAreaPaint
            })
            .onNodeClick(node => {
                // Center/zoom on node
                Graph.centerAt(node.x, node.y, 1000);
                Graph.zoom(8, 2000)
            })
            .onNodeHover(node => {
                elem.style.cursor = node ? 'pointer' : null
            })
            .nodePointerAreaPaint((node, color, ctx) => {
                ctx.fillStyle = color;
                const bckgDimensions = node.__bckgDimensions;
                bckgDimensions && ctx.fillRect(node.x - bckgDimensions[0] / 2, node.y - bckgDimensions[1] / 2, ...bckgDimensions);
            });
    });
